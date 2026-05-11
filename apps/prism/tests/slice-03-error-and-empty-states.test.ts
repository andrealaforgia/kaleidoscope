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
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { queryRange } from '../src/lib/promql/client';
import type { QueryOutcome } from '../src/lib/promql/types';
import { QueryPanel } from '../src/panels/query/QueryPanel';
import { loadConfig } from '../src/lib/config/loader';

import promqlSuccessFixture from './fixtures/promql-success.json' with { type: 'json' };
import promqlParseErrorFixture from './fixtures/promql-parse-error.json' with { type: 'json' };
import promqlEmptyFixture from './fixtures/promql-empty.json' with { type: 'json' };

// =============================================================================
// US-PR-03 AC-3.2 — PromQL parse error renders inline; URL preserved
// =============================================================================

describe('Slice 03 parse error — when the backend rejects my query (400 + status:error)', () => {
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
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I have loaded Prism with a fakeFetch returning the parse-error fixture
    // WHEN I type "rate(metric_name[5m" and press Run
    // THEN a warning banner appears with text: "1:48: parse error: unclosed left bracket"
    // AND the chart area shows "Backend rejected this query."
    // AND the query input still contains "rate(metric_name[5m"
    // AND the query input is still focusable
  });

  it('keeps the URL encoding the broken query so it is shareable (AC-3.2, AC-4.1, KPI 5)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I typed and submitted a broken query
    // WHEN the parse-error renders
    // THEN window.location.search contains "q=rate(metric_name%5B5m"
    //     (URL-encoded but lossless — a colleague pasting the URL sees the same broken state)
  });
});

// =============================================================================
// US-PR-03 AC-3.3 — transport error renders backend label and last-fetch time
// =============================================================================

describe('Slice 03 transport error — when the backend is unreachable', () => {
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
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I have loaded Prism with backend.label="dev-local-prom"
    // AND a fakeFetch that rejects (transport network failure)
    // WHEN I press Run
    // THEN a warning banner appears with text matching "dev-local-prom"
    // AND the banner names the transport-level error class
  });

  it('shows "Last successful fetch: ${last_fetch_time}" when a previous fetch succeeded (AC-3.3)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I have rendered a chart successfully at time T (capturing last_fetch_time)
    // AND now the backend is unreachable
    // WHEN I press Run
    // THEN the body region shows "Last successful fetch: ${T}"
    // (formatted as ISO-8601)
  });

  it('drops the previous chart on transport error — no stale-data lying (AC-3.5)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I have rendered a chart successfully (chart canvas in DOM)
    // AND now the backend is unreachable
    // WHEN I press Run
    // THEN the chart canvas is removed from the DOM
    // (the previous successful chart is NOT shown alongside the warning banner)
  });
});

// =============================================================================
// US-PR-03 AC-3.4 — empty result is calm, not alarming
// =============================================================================

describe('Slice 03 empty result — when the backend returns an empty data.result', () => {
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
    // Critical discriminator: empty is distinct from success-with-zero-series.
    // The QueryPanel renders different UI per arm (calm "No data" vs empty chart).
  });

  it('renders the calm empty-state message, NOT a warning banner (AC-3.4)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I have loaded Prism with a fakeFetch returning the empty fixture
    // WHEN I type a valid PromQL and press Run
    // THEN the chart area shows "No data for ${time_range_iso}. Check the metric name or widen the range."
    // AND there is NO warning banner anywhere on the page
  });

  it('keeps the URL encoded with the (empty-yielding) query (AC-3.4, AC-4.1)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I have typed and submitted a query that returns empty
    // WHEN the empty state renders
    // THEN window.location.search contains q=<my query> for shareability
  });
});

// =============================================================================
// US-PR-03 AC-3.5 — cross-mode invariant: never show stale chart with error
// =============================================================================

describe('Slice 03 stale-data invariant — when a successful chart precedes a failure', () => {
  it('removes the chart canvas before rendering a transport-error banner (AC-3.5)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I rendered a chart at time T (3 series visible)
    // WHEN the next fetch returns a transport-error
    // THEN the chart canvas is no longer in the DOM
    // AND the warning banner is shown
    // AND the body shows "Last successful fetch: ${T}"
  });

  it('keeps the chart canvas on parse-error (parse-error is "your query was wrong", not "data is stale")', async () => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I rendered a chart successfully
    // WHEN the next fetch returns a parse-error
    // THEN the chart area shows "Backend rejected this query." (calm fallback)
    // AND the previous chart is replaced with the calm message
    //     (per slice-03 brief: parse-error has its own calm fallback in the chart area)
  });
});

// =============================================================================
// US-PR-06 AC-6.2 — /config.json unreachable: composition root refuses to mount
// =============================================================================

describe('Slice 03 config error — when /config.json is unreachable', () => {
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
    // Note: Scholar's comment named the error kind "schema-invalid";
    // the canonical type in src/lib/config/types.ts is "shape-failed"
    // (ADR-0030 names the three ConfigError arms as fetch-failed /
    // parse-failed / shape-failed). The test asserts the canonical name.
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
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN /config.json returns 404
    // WHEN main.tsx-equivalent runs
    // THEN the page shows "Configuration is missing. Contact your Prism administrator."
    // AND the chrome backend label reads "(unconfigured)"
    // AND no fetch to /api/v1/query_range is attempted
  });
});

// =============================================================================
// US-PR-04 + KPI 5 — malformed URL fallback (ADR-0028 § 7)
// =============================================================================

describe('Slice 03 malformed URL — when a hand-edited URL has invalid parameters (KPI 5)', () => {
  it('renders the calm "Some URL parameters were invalid" banner (AC-3 family + KPI 5)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN I open Prism at "/?q=up&from=garbage&refresh=2s"
    // WHEN the page loads
    // THEN the malformed-URL banner appears at the top of the chrome
    // AND the banner names the invalid fields: "from, refresh"
    // AND the picker shows the default "Last 15 min"
    // AND the auto-refresh picker shows "off"
    // AND the page is fully interactive
  });

  it('dismisses the malformed-URL banner when I make any picker change', async () => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN the malformed-URL banner is showing
    // WHEN I open the time-range picker and pick "Last 1 h"
    // THEN the banner is dismissed
    // AND the URL is rewritten cleanly
  });
});

// =============================================================================
// Header redaction invariant (ADR-0027 § 6)
// =============================================================================

describe('Slice 03 header redaction — when the operator configured backend.headers', () => {
  it('does not leak header values into any QueryOutcome field, on any kind (ADR-0027 § 6)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 03 DELIVER');
    // GIVEN backend.headers = { "Authorization": "Bearer SECRET-TOKEN" }
    // AND a fakeFetch that includes the headers in its echoed response (worst case)
    // WHEN I call queryRange across all five outcome.kind values
    // THEN the JSON-stringified outcome NEVER contains the substring "SECRET-TOKEN"
    // (even when the backend's response or error text would have leaked it)
  });
});
