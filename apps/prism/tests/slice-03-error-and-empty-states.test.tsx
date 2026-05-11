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

// Slice 03 — Error and empty states.
//
// I am Priya. I am triaging at 03:14. The page must NEVER blank on me.
// A typo in my query must show me what the backend said. A backend
// outage must say which backend and what it said. An empty result
// must not look like an alarm. The URL must always still encode the
// state I am in, so I can paste it into Slack and a teammate sees the
// same broken view I see.
//
// Stories: US-PR-03 (errors + empty), US-PR-06 (config error), US-PR-04 (URL preserved).
// KPIs anchored: KPI 5 (page-stays-usable rendering arms — Vitest layer).
// ADRs: 0027 (QueryOutcome union), 0028 (malformed URL banner).

import { describe, expect, it, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

// userEvent.type treats {, [, ], } as keyboard modifier syntax. For
// tests that need to type literal PromQL fragments containing those
// characters (selectors, ranges), use fireEvent.change to set the
// input value directly. The Run button click then submits the form.
function setQueryInput(value: string): void {
  const input = screen.getByTestId('query-input') as HTMLInputElement;
  fireEvent.change(input, { target: { value } });
}

import { queryRange } from '../src/lib/promql/client';
import type { QueryOutcome, QueryRangeContext } from '../src/lib/promql/types';
import { QueryPanel } from '../src/panels/query/QueryPanel';
import { App } from '../src/app/App';
import { loadConfig } from '../src/lib/config/loader';

import promqlSuccessFixture from './fixtures/promql-success.json' with { type: 'json' };
import promqlParseErrorFixture from './fixtures/promql-parse-error.json' with { type: 'json' };
import promqlEmptyFixture from './fixtures/promql-empty.json' with { type: 'json' };

const TEST_CONFIG = {
  backend: { url: '/api/v1', label: 'dev-local-prom' },
  prism: { version: '0.1.0' },
} as const;

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

// =============================================================================
// US-PR-03 AC-3.2 — PromQL parse error renders inline; URL preserved
// =============================================================================

describe('Slice 03 parse error — when the backend rejects my query (400 + status:error)', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('classifies a 400 with status:error body as QueryOutcome.parse-error (ADR-0027 § 4)', async () => {
    const fakeFetch: typeof fetch = async () =>
      new Response(JSON.stringify(promqlParseErrorFixture), {
        status: 400,
        headers: { 'content-type': 'application/json' },
      });
    const outcome = await queryRange(
      { q: 'rate(metric_name[5m', range: { kind: 'relative', from: '-15m' } },
      { backend: '/api/v1', fetchFn: fakeFetch },
    );
    expect(outcome.kind).toBe('parse-error');
    if (outcome.kind === 'parse-error') {
      expect(outcome.backendError).toBe('1:48: parse error: unclosed left bracket');
    }
  });

  it('renders the verbatim backend error in an inline warning banner (AC-3.2)', async () => {
    const fakeFetch: typeof fetch = async () =>
      new Response(JSON.stringify(promqlParseErrorFixture), {
        status: 400,
        headers: { 'content-type': 'application/json' },
      });
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    const input = screen.getByTestId('query-input') as HTMLInputElement;
    setQueryInput('rate(metric_name[5m');
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('parse-error-banner')).not.toBeNull();
    });
    const banner = screen.getByTestId('parse-error-banner');
    expect(banner.textContent).toContain('1:48: parse error: unclosed left bracket');
    expect(banner.textContent).toContain('Backend rejected this query.');
    expect(input.value).toBe('rate(metric_name[5m');
    // The query input remains focusable.
    input.focus();
    expect(document.activeElement).toBe(input);
  });

  it('keeps the URL encoding the broken query so it is shareable (AC-3.2, AC-4.1, KPI 5)', async () => {
    const fakeFetch: typeof fetch = async () =>
      new Response(JSON.stringify(promqlParseErrorFixture), {
        status: 400,
        headers: { 'content-type': 'application/json' },
      });
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    setQueryInput('rate(metric_name[5m');
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('parse-error-banner')).not.toBeNull();
    });
    // URL-encoded "[" is %5B — a colleague pasting this URL sees the
    // exact same broken state. URLSearchParams encodes "(" as itself.
    expect(window.location.search).toContain('q=rate%28metric_name%5B5m');
  });
});

// =============================================================================
// US-PR-03 AC-3.3 — transport error renders backend label and last-fetch time
// =============================================================================

describe('Slice 03 transport error — when the backend is unreachable', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('classifies a fetch rejection as transport-error.network (ADR-0027 § 3)', async () => {
    const fakeFetch: typeof fetch = async () => {
      throw new TypeError('Failed to fetch');
    };
    const outcome = await queryRange(
      { q: 'up', range: { kind: 'relative', from: '-15m' } },
      { backend: '/api/v1', fetchFn: fakeFetch },
    );
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error' && outcome.cause.kind === 'network') {
      expect(outcome.cause.message).toContain('Failed to fetch');
    } else {
      throw new Error(`expected transport-error.network, got ${JSON.stringify(outcome)}`);
    }
  });

  it('classifies an HTTP 500 as transport-error.http-status (ADR-0027 § 3)', async () => {
    const fakeFetch: typeof fetch = async () =>
      new Response('internal server error', {
        status: 500,
        headers: { 'content-type': 'text/plain' },
      });
    const outcome = await queryRange(
      { q: 'up', range: { kind: 'relative', from: '-15m' } },
      { backend: '/api/v1', fetchFn: fakeFetch },
    );
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error' && outcome.cause.kind === 'http-status') {
      expect(outcome.cause.status).toBe(500);
    } else {
      throw new Error(`expected transport-error.http-status, got ${JSON.stringify(outcome)}`);
    }
  });

  it('classifies a 200 with non-JSON body as transport-error.invalid-json (ADR-0027 § 3)', async () => {
    const fakeFetch: typeof fetch = async () =>
      new Response('not actually json', {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    const outcome = await queryRange(
      { q: 'up', range: { kind: 'relative', from: '-15m' } },
      { backend: '/api/v1', fetchFn: fakeFetch },
    );
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('invalid-json');
    }
  });

  it('classifies a 200 with JSON missing data.result as transport-error.shape (ADR-0027 § 3)', async () => {
    const fakeFetch: typeof fetch = async () =>
      new Response(JSON.stringify({ status: 'success' }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    const outcome = await queryRange(
      { q: 'up', range: { kind: 'relative', from: '-15m' } },
      { backend: '/api/v1', fetchFn: fakeFetch },
    );
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('shape');
    }
  });

  it('renders an inline warning naming the backend label (AC-3.3)', async () => {
    const fakeFetch: typeof fetch = async () => {
      throw new TypeError('Failed to fetch');
    };
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    await user.type(screen.getByTestId('query-input'), 'up');
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('transport-error-banner')).not.toBeNull();
    });
    const banner = screen.getByTestId('transport-error-banner');
    expect(banner.textContent).toContain('dev-local-prom');
    // The banner names the transport-level error class — "network"
    // for a fetch rejection (ADR-0027 §3).
    expect(banner.textContent).toContain('network');
  });

  it('shows "Last successful fetch: ${last_fetch_time}" when a previous fetch succeeded (AC-3.3)', async () => {
    // First fetch succeeds (records last_fetch_time); next fetch fails.
    let callCount = 0;
    const fakeFetch: typeof fetch = async () => {
      callCount += 1;
      if (callCount === 1) return jsonResponse(promqlSuccessFixture);
      throw new TypeError('Failed to fetch');
    };
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    const tBefore = Date.now();
    await user.type(screen.getByTestId('query-input'), 'up');
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('chart-footer')?.textContent ?? '').toContain('series');
    });
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('transport-error-banner')).not.toBeNull();
    });
    const lastFetch = screen.getByTestId('last-fetch-time');
    expect(lastFetch.textContent).toMatch(/^Last successful fetch: \d{4}-\d{2}-\d{2}T/);
    // The recorded timestamp is the one captured at success.
    const isoMatch = lastFetch.textContent!.match(/\d{4}-\d{2}-\d{2}T[\d:.]+Z/);
    expect(isoMatch).not.toBeNull();
    const recorded = new Date(isoMatch![0]).getTime();
    expect(recorded).toBeGreaterThanOrEqual(tBefore);
  });

  it('drops the previous chart on transport error — no stale-data lying (AC-3.5)', async () => {
    let callCount = 0;
    const fakeFetch: typeof fetch = async () => {
      callCount += 1;
      if (callCount === 1) return jsonResponse(promqlSuccessFixture);
      throw new TypeError('Failed to fetch');
    };
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    await user.type(screen.getByTestId('query-input'), 'up');
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('chart-canvas')).not.toBeNull();
    });
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('transport-error-banner')).not.toBeNull();
    });
    // The chart canvas is removed from the DOM, not merely hidden.
    expect(screen.queryByTestId('chart-canvas')).toBeNull();
  });
});

// =============================================================================
// US-PR-03 AC-3.4 — empty result is calm, not alarming
// =============================================================================

describe('Slice 03 empty result — when the backend returns an empty data.result', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('classifies a 200 with empty data.result as QueryOutcome.empty (ADR-0027 § 4)', async () => {
    const fakeFetch: typeof fetch = async () =>
      new Response(JSON.stringify(promqlEmptyFixture), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    const outcome = await queryRange(
      { q: 'up{job="nonexistent_job"}', range: { kind: 'relative', from: '-15m' } },
      { backend: '/api/v1', fetchFn: fakeFetch },
    );
    expect(outcome.kind).toBe('empty');
  });

  it('renders the calm empty-state message, NOT a warning banner (AC-3.4)', async () => {
    const fakeFetch: typeof fetch = async () => jsonResponse(promqlEmptyFixture);
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    setQueryInput('up{job="nonexistent"}');
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('empty-state')).not.toBeNull();
    });
    const empty = screen.getByTestId('empty-state');
    expect(empty.textContent).toContain('No data for');
    // The active range is rendered in the empty message (default
    // "last 15 minutes" here).
    expect(empty.textContent).toContain('last 15 minutes');
    expect(empty.textContent).toContain('Check the metric name or widen the range.');
    // No warning banners on the page on the empty path.
    expect(screen.queryByTestId('parse-error-banner')).toBeNull();
    expect(screen.queryByTestId('transport-error-banner')).toBeNull();
  });

  it('keeps the URL encoded with the (empty-yielding) query (AC-3.4, AC-4.1)', async () => {
    const fakeFetch: typeof fetch = async () => jsonResponse(promqlEmptyFixture);
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    await user.type(screen.getByTestId('query-input'), 'up_nonexistent');
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('empty-state')).not.toBeNull();
    });
    expect(window.location.search).toContain('q=up_nonexistent');
  });
});

// =============================================================================
// US-PR-03 AC-3.5 — cross-mode invariant: never show stale chart with error
// =============================================================================

describe('Slice 03 stale-data invariant — when a successful chart precedes a failure', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('removes the chart canvas before rendering a transport-error banner (AC-3.5)', async () => {
    let callCount = 0;
    const fakeFetch: typeof fetch = async () => {
      callCount += 1;
      if (callCount === 1) return jsonResponse(promqlSuccessFixture);
      throw new TypeError('Failed to fetch');
    };
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    await user.type(screen.getByTestId('query-input'), 'up');
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('chart-canvas')).not.toBeNull();
    });
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('transport-error-banner')).not.toBeNull();
    });
    expect(screen.queryByTestId('chart-canvas')).toBeNull();
    expect(screen.queryByTestId('last-fetch-time')).not.toBeNull();
  });

  it('replaces the chart with the calm parse-error fallback (parse-error is "your query was wrong")', async () => {
    let callCount = 0;
    const fakeFetch: typeof fetch = async () => {
      callCount += 1;
      if (callCount === 1) return jsonResponse(promqlSuccessFixture);
      return new Response(JSON.stringify(promqlParseErrorFixture), {
        status: 400,
        headers: { 'content-type': 'application/json' },
      });
    };
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const user = userEvent.setup();
    await user.type(screen.getByTestId('query-input'), 'up');
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('chart-canvas')).not.toBeNull();
    });
    await user.click(screen.getByTestId('run-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('parse-error-banner')).not.toBeNull();
    });
    expect(screen.queryByTestId('chart-canvas')).toBeNull();
    expect(screen.queryByTestId('parse-error-fallback')?.textContent).toContain(
      'Backend rejected this query.',
    );
  });
});

// =============================================================================
// US-PR-06 AC-6.2 — /config.json unreachable: composition root refuses to mount
// =============================================================================

describe('Slice 03 config error — when /config.json is unreachable', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('returns a typed ConfigError when fetch rejects (AC-6.2)', async () => {
    const fakeFetch: typeof fetch = async () => {
      throw new TypeError('Failed to fetch');
    };
    const result = await loadConfig({ fetchFn: fakeFetch });
    expect(result.kind).toBe('error');
    if (result.kind === 'error') {
      expect(result.error.kind).toBe('fetch-failed');
    }
  });

  it('returns a typed ConfigError when /config.json returns 404 (AC-6.2)', async () => {
    const fakeFetch: typeof fetch = async () => new Response('not found', { status: 404 });
    const result = await loadConfig({ fetchFn: fakeFetch });
    expect(result.kind).toBe('error');
    if (result.kind === 'error') {
      expect(result.error.kind).toBe('fetch-failed');
      expect(result.error.message).toContain('404');
    }
  });

  it('returns a typed ConfigError when /config.json is malformed JSON (AC-6.2)', async () => {
    const fakeFetch: typeof fetch = async () =>
      new Response('{ this is not json', {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    const result = await loadConfig({ fetchFn: fakeFetch });
    expect(result.kind).toBe('error');
    if (result.kind === 'error') {
      expect(result.error.kind).toBe('parse-failed');
    }
  });

  it('returns a typed ConfigError when backend.url is missing (AC-6.2)', async () => {
    const fakeFetch: typeof fetch = async () =>
      new Response(JSON.stringify({ backend: { label: 'x' } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    const result = await loadConfig({ fetchFn: fakeFetch });
    expect(result.kind).toBe('error');
    if (result.kind === 'error') {
      expect(result.error.kind).toBe('shape-failed');
    }
  });

  it('renders the calm "Configuration is missing" banner without mounting QueryPanel (AC-6.2)', async () => {
    const calls: string[] = [];
    const fakeFetch: typeof fetch = async (input) => {
      const url = typeof input === 'string' ? input : (input as URL).toString();
      calls.push(url);
      if (url.endsWith('/config.json')) return new Response('not found', { status: 404 });
      // No other fetch should happen — assert at end.
      return jsonResponse(promqlSuccessFixture);
    };
    render(<App fetchFn={fakeFetch} />);
    await waitFor(() => {
      expect(screen.queryByTestId('config-error-banner')).not.toBeNull();
    });
    const banner = screen.getByTestId('config-error-banner');
    expect(banner.textContent).toContain('Configuration is missing.');
    expect(banner.textContent).toContain('Contact your Prism administrator.');
    expect(screen.getByTestId('backend-label').textContent).toContain('(unconfigured)');
    // No query_range fetch was attempted.
    expect(calls.filter((c) => c.includes('/query_range'))).toEqual([]);
    expect(screen.queryByTestId('query-panel')).toBeNull();
  });
});

// =============================================================================
// US-PR-04 + KPI 5 — malformed URL fallback (ADR-0028 § 7)
// =============================================================================

describe('Slice 03 malformed URL — when a hand-edited URL has invalid parameters (KPI 5)', () => {
  it('renders the calm "Some URL parameters were invalid" banner (AC-3 family + KPI 5)', () => {
    window.history.replaceState({}, '', '/?q=up&from=garbage&refresh=2s');
    const fakeFetch: typeof fetch = async () => jsonResponse(promqlSuccessFixture);
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    const banner = screen.getByTestId('malformed-url-banner');
    expect(banner.textContent).toContain('Some URL parameters were invalid.');
    expect(banner.textContent).toContain('from, refresh');
    // Picker defaults to "Last 15 min".
    const picker = screen.getByTestId('time-range-picker') as HTMLSelectElement;
    expect(picker.value).toBe('-15m');
    // The page is fully interactive: query input is focusable and the
    // Run button is present (disabled because q was reset to default
    // when only invalid params survive — but the input lets us type).
    const input = screen.getByTestId('query-input') as HTMLInputElement;
    expect(input).not.toBeNull();
    input.focus();
    expect(document.activeElement).toBe(input);
  });

  it('dismisses the malformed-URL banner when I make any picker change', async () => {
    window.history.replaceState({}, '', '/?q=up&from=garbage&refresh=2s');
    const fakeFetch: typeof fetch = async () => jsonResponse(promqlSuccessFixture);
    render(<QueryPanel config={TEST_CONFIG} fetchFn={fakeFetch} />);
    expect(screen.queryByTestId('malformed-url-banner')).not.toBeNull();
    const user = userEvent.setup();
    await user.selectOptions(screen.getByTestId('time-range-picker'), '-1h');
    await waitFor(() => {
      expect(screen.queryByTestId('malformed-url-banner')).toBeNull();
    });
    // URL is rewritten cleanly: only canonical parameters.
    expect(window.location.search).toContain('from=-1h');
    expect(window.location.search).toContain('to=now');
    expect(window.location.search).not.toContain('garbage');
    expect(window.location.search).not.toContain('refresh=2s');
  });
});

// =============================================================================
// Header redaction invariant (ADR-0027 § 6)
// =============================================================================

describe('Slice 03 header redaction — when the operator configured backend.headers', () => {
  it('does not leak header values into any QueryOutcome field, on any kind (ADR-0027 § 6)', async () => {
    const SECRET = 'SECRET-TOKEN';
    const headers = { Authorization: `Bearer ${SECRET}` };

    // Build one fakeFetch per outcome kind, each crafted so the
    // header value WOULD leak if redaction were absent: the body
    // text, the prom-error message, the network exception, the
    // success-arm label all carry the secret. After redaction, the
    // JSON-stringified outcome must never contain the substring.
    const cases: ReadonlyArray<{
      readonly kind: QueryOutcome['kind'];
      readonly fetch: typeof fetch;
    }> = [
      {
        kind: 'success',
        fetch: async () =>
          jsonResponse({
            status: 'success',
            data: {
              resultType: 'matrix',
              result: [
                {
                  metric: { __name__: 'up', auth_leak: SECRET },
                  values: [[1700000000, '1']],
                },
              ],
            },
          }),
      },
      {
        kind: 'empty',
        fetch: async () =>
          jsonResponse({ status: 'success', data: { resultType: 'matrix', result: [] } }),
      },
      {
        kind: 'parse-error',
        fetch: async () =>
          new Response(JSON.stringify({ status: 'error', error: `parse failed near ${SECRET}` }), {
            status: 400,
            headers: { 'content-type': 'application/json' },
          }),
      },
      {
        kind: 'transport-error',
        fetch: async () => {
          throw new TypeError(`Failed to fetch ${SECRET}`);
        },
      },
    ];

    const ctxBase = { backend: '/api/v1', headers } as const;

    for (const c of cases) {
      const ctx: QueryRangeContext = { ...ctxBase, fetchFn: c.fetch };
      const outcome = await queryRange({ q: 'up', range: { kind: 'relative', from: '-15m' } }, ctx);
      expect(outcome.kind).toBe(c.kind);
      const serialised = JSON.stringify(outcome);
      expect(serialised.includes(SECRET)).toBe(false);
    }

    // Additional case: HTTP 500 with body echoing the secret.
    const httpOutcome = await queryRange(
      { q: 'up', range: { kind: 'relative', from: '-15m' } },
      {
        ...ctxBase,
        fetchFn: async () =>
          new Response(`auth header was ${SECRET}`, {
            status: 500,
            headers: { 'content-type': 'text/plain' },
          }),
      },
    );
    expect(httpOutcome.kind).toBe('transport-error');
    expect(JSON.stringify(httpOutcome).includes(SECRET)).toBe(false);
  });
});
