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

// Slice 08 — the logs search view + pivot, cold flow in a REAL browser.
//
// I am a newcomer. A symptom is loose in the system. I open Prism, switch
// to the Logs view, and search the log body for the symptom text. The
// matching logs come back; one of them carries a trace_id, so it offers a
// pivot. I click the pivot and land — on the SAME app, deep-linked to
// /traces?trace=<id> — on the existing linked-view detail, which has
// AUTO-OPENED the trace: the error span's status message (WHERE it broke)
// next to the correlated "card declined" cause log (WHY). One flow, from
// "what I saw" to "where + why", no second tab.
//
// -----------------------------------------------------------------------
// BACKEND-PROVISIONING CHOICE — APPROACH 2 (page.route interception),
// mirroring slice 07. The Prism e2e harness serves the REAL Vite-built
// SPA (`pnpm dev`) and provisions ONLY a Prometheus container for the
// metrics slices; there is no Rust log/trace backend in this harness. So
// the riskiest new part — the SPA RENDERING of the logs view and the
// pivot in a real browser — is proven with the real served SPA while
// Playwright `page.route` fulfils the same-origin `/api/v1/logs` and
// `/api/v1/traces/with_logs` calls with canned JSON faithful to the wire
// shapes (lib/logs/types.ts, lib/traces/types.ts). The backend contract
// itself is verified independently (Rust-side conformance + the clients'
// own vitest suites over these shapes).
// -----------------------------------------------------------------------

import { test, expect, type Page } from '@playwright/test';

const SERVICE = 'demo-checkout';
const FAILED_TRACE_ID = '1'.repeat(32);

const SYMPTOM = 'card declined';
const WHERE_MESSAGE = 'checkout failed: card declined';
const WHY_LOG_BODY = 'card declined: insufficient funds';

const BASE_MS = 1_733_000_000_000;
const nanos = (offsetMs: number): number => (BASE_MS + offsetMs) * 1_000_000;

interface WireLog {
  observed_time_unix_nano: number;
  severity_number: number;
  severity_text: string;
  body: string;
  attributes: Record<string, string>;
  resource_attributes: Record<string, string>;
  trace_id?: string;
  span_id?: string;
}

// The symptom search returns a flat LogView[]: one correlated ERROR log
// (carries trace_id → pivotable) and one bare WARN log (no trace → an
// honest dead end, no pivot).
const symptomLogs: WireLog[] = [
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
  {
    observed_time_unix_nano: nanos(50),
    severity_number: 13,
    severity_text: 'WARN',
    body: 'card declined retry scheduled (no trace attached)',
    attributes: {},
    resource_attributes: { 'service.name': SERVICE },
  },
];

// /traces/with_logs for the pivoted trace: the spans (with the Error
// status + readable WHERE message) AND the correlated cause log (WHY).
const failedWithLogs = {
  trace_id: FAILED_TRACE_ID,
  spans: [
    {
      trace_id: FAILED_TRACE_ID,
      span_id: 'a1a1a1a1a1a1a1a1',
      name: 'POST /api/v1/checkout',
      kind: 'Server',
      start_time_unix_nano: nanos(0),
      end_time_unix_nano: nanos(120),
      status: { code: 'Error', message: WHERE_MESSAGE },
      attributes: {},
      resource_attributes: { 'service.name': SERVICE },
      events: [],
      links: [],
    },
  ],
  logs: [
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

async function stubLogsAndTracesBackend(page: Page): Promise<void> {
  const json = (route: Parameters<Parameters<Page['route']>[1]>[0], body: unknown): Promise<void> =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });

  // The logs symptom search.
  await page.route(/\/api\/v1\/logs/, async (route) => json(route, symptomLogs));
  // The pivot landing fetches the trace with its correlated logs.
  await page.route(/\/api\/v1\/traces/, async (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/traces/with_logs')) return json(route, failedWithLogs);
    return json(route, []);
  });
}

test.describe('Slice 08 logs search + pivot — from the symptom to WHERE+WHY on one screen', () => {
  let pageErrors: Error[] = [];

  test.beforeEach(async ({ page }) => {
    pageErrors = [];
    page.on('pageerror', (err) => pageErrors.push(err));
    await stubLogsAndTracesBackend(page);
  });

  test.afterEach(() => {
    expect(pageErrors).toEqual([]);
  });

  test('newcomer searches the logs for a symptom, pivots from the matching log, and lands on the trace WHERE+WHY', async ({
    page,
  }) => {
    // GIVEN I open Prism and switch to the Logs view via the nav.
    await page.goto('/');
    await page.getByTestId('nav-logs').click();
    await expect(page.getByTestId('logs-explorer-panel')).toBeVisible();
    expect(new URL(page.url()).pathname).toBe('/logs');

    // WHEN I search the log body for the symptom text.
    await expect(page.getByTestId('log-body-input')).toBeVisible();
    await page.getByTestId('log-body-input').fill(SYMPTOM);
    await page.getByTestId('log-run-button').click();

    // THEN the matching logs come back — both carry the symptom, but only
    // the correlated one offers a pivot.
    const rows = page.getByTestId('log-row');
    await expect(rows).toHaveCount(2);
    await expect(page.getByTestId('log-pivot')).toHaveCount(1);

    // AND a screen-reader user is told the result arrived (WCAG 2.2 AA — 4.1.3).
    const logsStatus = page.getByTestId('logs-status');
    await expect(logsStatus).toHaveAttribute('role', 'status');
    await expect(logsStatus).toHaveAttribute('aria-live', 'polite');
    await expect(logsStatus).toContainText('2 logs found');

    // WHEN I pivot from the matching log.
    await page.getByTestId('log-pivot').click();

    // THEN I am deep-linked to /traces carrying the trace_id, and the
    // linked-view detail has AUTO-OPENED the trace — no manual find.
    await expect(page).toHaveURL(new RegExp(`/traces\\?trace=${FAILED_TRACE_ID}`));
    const detailRegion = page.getByTestId('trace-detail-region');
    const detailBody = detailRegion.getByTestId('trace-detail');
    await expect(detailBody).toBeVisible();

    // WHERE — the error span's readable status message.
    const whereMessage = detailBody.getByTestId('span-status-message');
    await expect(whereMessage).toBeVisible();
    await expect(whereMessage).toContainText(WHERE_MESSAGE);

    // WHY — the correlated "card declined" cause log, in the SAME region.
    const whyLog = detailBody.getByTestId('cause-log');
    await expect(whyLog).toBeVisible();
    await expect(whyLog).toContainText('card declined');

    // AND the payoff was on ONE screen: still in the same app, no new tab.
    expect(new URL(page.url()).pathname).toBe('/traces');
    expect(page.context().pages()).toHaveLength(1);
  });
});
