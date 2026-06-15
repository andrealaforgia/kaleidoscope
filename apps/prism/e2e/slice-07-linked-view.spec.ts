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

// Slice 07 — the linked view, cold flow in a REAL browser.
//
// I am a newcomer. I open Prism and switch to the Traces view. The
// "errors only" toggle is OFF by default, so my FIRST search of the
// demo-checkout service over the default window shows the WHOLE picture —
// the failed checkout sitting AMONG the three healthy traces, with the
// failed one already badged Error. The single badge among the four rows
// proves the failure is distinguishable on sight WITHOUT opening every
// trace. I then flip "errors only" ON — the one-click "show me problems
// first" — and the list narrows to exactly the failed checkout. I click
// it and, on the SAME screen (no second tab, no navigation), I read BOTH:
// the error span's status message "checkout failed: card declined" (WHERE
// it broke) and the "card declined" cause log (WHY) — together, in one
// region.
//
// -----------------------------------------------------------------------
// BACKEND-PROVISIONING CHOICE — APPROACH 2 (page.route interception).
//
// The brief's PREFERRED approach (drive the real served SPA against a
// live consolidated runtime / seeded backend) is NOT how THIS harness
// works: playwright.config.ts > webServer runs `pnpm dev` (the real Vite-
// served Prism SPA) and globalSetup provisions ONLY a Prometheus
// container for the metrics slices. There is no Rust traces backend, no
// consolidated-runtime, and no trace-seed step anywhere in the Prism e2e
// harness. Standing one up here would be a new, out-of-band fixture.
//
// So we take the ACCEPTABLE fallback: the REAL served SPA (the same Vite
// build the metrics slices drive) rendered in a real browser, with
// Playwright `page.route` fulfilling the three same-origin trace calls
// (`/api/v1/traces?...error=true`, the unfiltered `/api/v1/traces?...`,
// and `/api/v1/traces/with_logs?...`) with canned JSON that is faithful
// to the real wire shapes (ray::Span flat array for find; {trace_id,
// spans, logs} for with_logs). This proves the riskiest new part — the
// SPA RENDERING of the linked view in a real browser across the three
// engines. The backend contract itself is verified independently
// (Rust-side conformance + the client's own vitest suite over these
// shapes). The canned data models the demo contract: one failed
// `POST /api/v1/checkout` (Error status + readable message + correlated
// "card declined" ERROR-severity cause log) plus three healthy Ok traces.
// -----------------------------------------------------------------------

import { test, expect, type Page } from '@playwright/test';

// ---------------------------------------------------------------------------
// Demo contract — faithful to the backend wire shapes (lib/traces/types.ts).
// ---------------------------------------------------------------------------

const SERVICE = 'demo-checkout';
const FAILED_TRACE_ID = '1'.repeat(32);
const HEALTHY_TRACE_IDS = ['2'.repeat(32), '3'.repeat(32), '4'.repeat(32)] as const;

const WHERE_MESSAGE = 'checkout failed: card declined';
const WHY_LOG_BODY = 'card declined: insufficient funds';

// Recent, deterministic epoch-nanosecond window (anchored, not Date.now,
// so the canned payload is byte-stable across runs/engines).
const BASE_MS = 1_733_000_000_000; // 2024-12-01-ish; well-formed nanos.
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

// The failed checkout: a root POST span carrying the Error status + the
// readable WHERE message, plus an Ok child (faithful, multi-span trace).
const failedSpans: WireSpan[] = [
  span({
    trace_id: FAILED_TRACE_ID,
    span_id: 'a1a1a1a1a1a1a1a1',
    name: 'POST /api/v1/checkout',
    status: { code: 'Error', message: WHERE_MESSAGE },
  }),
  span({
    trace_id: FAILED_TRACE_ID,
    span_id: 'a2a2a2a2a2a2a2a2',
    parent_span_id: 'a1a1a1a1a1a1a1a1',
    name: 'GET /api/v1/cart',
    status: { code: 'Ok', message: '' },
  }),
];

// Three healthy traces for the same service — Ok, single root span each.
const healthySpans: WireSpan[] = [
  span({
    trace_id: HEALTHY_TRACE_IDS[0],
    span_id: 'b1b1b1b1b1b1b1b1',
    name: 'GET /api/v1/products',
    status: { code: 'Ok', message: '' },
  }),
  span({
    trace_id: HEALTHY_TRACE_IDS[1],
    span_id: 'c1c1c1c1c1c1c1c1',
    name: 'POST /api/v1/checkout',
    status: { code: 'Ok', message: '' },
  }),
  span({
    trace_id: HEALTHY_TRACE_IDS[2],
    span_id: 'd1d1d1d1d1d1d1d1',
    name: 'GET /api/v1/cart',
    status: { code: 'Ok', message: '' },
  }),
];

// /traces/with_logs for the failed checkout: the spans AND the correlated
// logs. The "card declined" log carries OTel ERROR severity (number 17),
// so the SPA renders it as the cause-log (WHY). An INFO log corroborates
// that ordinary logs render too without being flagged a cause.
const failedWithLogs = {
  trace_id: FAILED_TRACE_ID,
  spans: failedSpans,
  logs: [
    {
      observed_time_unix_nano: nanos(10),
      severity_number: 9,
      severity_text: 'INFO',
      body: 'checkout started for cart #42',
      attributes: {},
      resource_attributes: { 'service.name': SERVICE },
      trace_id: FAILED_TRACE_ID,
      span_id: 'a1a1a1a1a1a1a1a1',
    },
    {
      observed_time_unix_nano: nanos(95),
      severity_number: 17,
      severity_text: 'ERROR',
      body: WHY_LOG_BODY,
      attributes: { 'payment.decline_code': 'insufficient_funds' },
      resource_attributes: { 'service.name': SERVICE },
      trace_id: FAILED_TRACE_ID,
      span_id: 'a1a1a1a1a1a1a1a1',
    },
  ],
};

// ---------------------------------------------------------------------------
// Same-origin trace API, served entirely from canned demo data. A single
// regex route branches on path + the `error` query param, so handler
// registration order is irrelevant.
// ---------------------------------------------------------------------------

async function stubTracesBackend(page: Page): Promise<void> {
  await page.route(/\/api\/v1\/traces/, async (route) => {
    const url = new URL(route.request().url());
    const json = (body: unknown): Promise<void> =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(body),
      });

    if (url.pathname.endsWith('/traces/with_logs')) {
      return json(failedWithLogs);
    }
    // The find surface returns a FLAT span array (ray::Span[]).
    if (url.searchParams.get('error') === 'true') {
      return json(failedSpans); // errors-only → just the failed checkout
    }
    return json([...failedSpans, ...healthySpans]); // all traces
  });
}

// ---------------------------------------------------------------------------
// The cold flow.
// ---------------------------------------------------------------------------

test.describe('Slice 07 linked view — the cold flow that finds WHERE+WHY together', () => {
  // Guardrail mirroring slice 03: no uncaught app error escapes during the
  // flow. (Failed-resource network noise is irrelevant here — every call is
  // fulfilled 200 by the route — so any console error would be a real bug.)
  let pageErrors: Error[] = [];

  test.beforeEach(async ({ page }) => {
    pageErrors = [];
    page.on('pageerror', (err) => pageErrors.push(err));
    await stubTracesBackend(page);
  });

  test.afterEach(() => {
    expect(pageErrors).toEqual([]);
  });

  test('newcomer opens Traces with errors-only OFF, sees the failure among the successes, narrows with the toggle, and reads WHERE + WHY on one screen', async ({
    page,
  }) => {
    // GIVEN I open Prism and switch to the Traces view via the nav.
    await page.goto('/');
    await page.getByTestId('nav-traces').click();
    await expect(page.getByTestId('trace-explorer-panel')).toBeVisible();
    expect(new URL(page.url()).pathname).toBe('/traces');

    // AND "errors only" is OFF by default — my FIRST view is the whole
    // picture, not failures-only. (This opening toggle state is client
    // state, not observable over HTTP, so we assert it programmatically.)
    await expect(page.getByTestId('errors-only-toggle')).not.toBeChecked();

    // WHEN I search the demo service over the default recent window.
    await page.getByTestId('trace-service-input').fill(SERVICE);
    await page.getByTestId('trace-run-button').click();

    // THEN the first paint shows the failure AMONG the successes — all four
    // traces, with exactly ONE Error badge on the failed checkout. The
    // failure is identifiable on sight WITHOUT opening every trace.
    const rows = page.getByTestId('trace-row');
    await expect(rows).toHaveCount(4);
    await expect(page.getByTestId('trace-error-badge')).toHaveCount(1);
    await expect(rows.filter({ has: page.getByTestId('trace-error-badge') })).toContainText(
      'POST /api/v1/checkout',
    );

    // AND flipping "errors only" ON — the one-click "show me problems
    // first" — and re-searching narrows to exactly the failed checkout.
    await page.getByTestId('errors-only-toggle').check();
    await page.getByTestId('trace-run-button').click();
    await expect(rows).toHaveCount(1);
    await expect(page.getByTestId('trace-error-badge')).toHaveCount(1);
    await expect(rows.first()).toContainText('POST /api/v1/checkout');

    // Open the failed checkout that is now the one row in front of me.
    await rows.first().click();

    // THEN — the linked payoff — in the SAME trace-detail region (no
    // navigation away from /traces, no second tab) BOTH facts are visible
    // together:
    const detailRegion = page.getByTestId('trace-detail-region');
    const detailBody = detailRegion.getByTestId('trace-detail');
    await expect(detailBody).toBeVisible();

    // WHERE — the error span's readable status message.
    const whereMessage = detailBody.getByTestId('span-status-message');
    await expect(whereMessage).toBeVisible();
    await expect(whereMessage).toContainText(WHERE_MESSAGE);
    // ...and it sits on a span-row inside this same detail region.
    await expect(detailBody.getByTestId('span-row').first()).toBeVisible();

    // WHY — the correlated "card declined" cause log, in the SAME region.
    const whyLog = detailBody.getByTestId('cause-log');
    await expect(whyLog).toBeVisible();
    await expect(whyLog).toContainText('card declined');
    // ...rendered as a log-row inside this same detail region.
    await expect(
      detailBody.getByTestId('log-row').filter({ hasText: 'card declined' }),
    ).toBeVisible();

    // AND the payoff was on ONE screen: still on /traces, no new tab opened.
    expect(new URL(page.url()).pathname).toBe('/traces');
    expect(page.context().pages()).toHaveLength(1);
  });
});
