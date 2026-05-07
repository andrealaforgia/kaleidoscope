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

// Slice 01 — Walking skeleton.
//
// Priya, the on-call SRE, opens Prism, types `up`, presses Run, and
// sees a chart of the points the backend returned — not smoothed, not
// interpolated, not aggregated. The page chrome names the backend.
// The URL bar updates so a teammate can paste it into Slack and see
// the same chart.
//
// Stories: US-PR-01, US-PR-02 (default 15-min only), US-PR-03 (fidelity),
// US-PR-04 (within-session reload + URL writes), US-PR-06 (chrome).
// KPIs anchored: KPI 3 (fidelity, also covered by invariant-fidelity.test.ts);
// KPI 4 (codec property — within-session reload).
// ADRs: 0026 (component layout), 0027 (HTTP client), 0028 (URL codec),
// 0030 (ECharts integration), 0032 (licence).

import { describe, expect, it, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

// Public types from the lib/ modules. ADR-0026 § 4 names them.
import { queryRange } from '../src/lib/promql/client';
import type { QueryOutcome, QueryRangeRequest, QueryRangeContext } from '../src/lib/promql/types';
import { decode, encode } from '../src/lib/url-state/codec';
import type { UrlState, RelativeOffset, RefreshInterval } from '../src/lib/url-state/types';
import { buildOption } from '../src/lib/echarts/buildOption';
import type { BuildOptionContext } from '../src/lib/echarts/buildOption';
import { loadConfig } from '../src/lib/config/loader';
import type { RuntimeConfig } from '../src/lib/config/types';
import { QueryPanel } from '../src/panels/query/QueryPanel';

import promqlSuccessFixture from './fixtures/promql-success.json' with { type: 'json' };

// =============================================================================
// US-PR-01 — query → chart
// =============================================================================

describe('Slice 01 walking skeleton — when I open Prism and type a query', () => {
  // -----------------------------------------------------------------
  // AC-1.1 — Pressing Enter or Run issues a single GET to query_range.
  // -----------------------------------------------------------------

  it('issues a single GET /api/v1/query_range when I press Enter on a non-empty query (AC-1.1)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN I have loaded a fresh Prism page with backend label "dev-local-prom"
    // AND the query input is focused with no query
    // const fetchCalls: { url: string; init?: RequestInit }[] = [];
    // const fetchFn: typeof fetch = async (url, init) => {
    //   fetchCalls.push({ url: String(url), init });
    //   return new Response(JSON.stringify(promqlSuccessFixture), { status: 200 });
    // };
    // const config: RuntimeConfig = {
    //   backend: { url: 'http://prom.test', label: 'dev-local-prom', headers: {} },
    // };
    // render(<QueryPanel config={config} fetchFn={fetchFn} />);
    //
    // // WHEN I type "up" and press Enter
    // const input = screen.getByLabelText(/PromQL query/i);
    // await userEvent.type(input, 'up{enter}');
    //
    // // THEN exactly one fetch is issued, against /api/v1/query_range, with q=up
    // await waitFor(() => expect(fetchCalls).toHaveLength(1));
    // const issued = new URL(fetchCalls[0].url);
    // expect(issued.pathname).toBe('/api/v1/query_range');
    // expect(issued.searchParams.get('query')).toBe('up');
    // expect(issued.searchParams.get('start')).toBeTruthy();
    // expect(issued.searchParams.get('end')).toBeTruthy();
  });

  it('treats clicking the Run button as equivalent to pressing Enter (AC-1.1)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN I have loaded a fresh Prism page
    // WHEN I type "up" and click the Run button
    // THEN exactly one fetch is issued (no double-fetch race with Enter)
  });

  it('disables the Run button when the query is empty (AC-1.1)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN a fresh Prism page, no query typed
    // WHEN the page renders
    // THEN the Run button is disabled (cannot be clicked)
  });

  // -----------------------------------------------------------------
  // AC-1.2 — Successful response renders as a line chart with one
  // series per labelled timeseries.
  // -----------------------------------------------------------------

  it('renders one chart series per timeseries the backend returned (AC-1.2)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN the backend will return 3 series (per fixture)
    // WHEN I run the query
    // THEN the chart's option has 3 series, in the same order as data.result
    // AND the legend names each series by its labels (instance=..., job=...)
    // AND no series is named "series-1" or "series-2"
  });

  it('shows the footer "<N> series · <M> points · fetched in <Q> ms" after a successful query (AC-1.2)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN the backend returned 3 series, 15 total points
    // WHEN the chart renders
    // THEN the footer reads "3 series · 15 points · fetched in <Q> ms"
    // (Q is a positive integer the queryRange call captures)
  });

  // -----------------------------------------------------------------
  // AC-1.3 — Chart points are exactly the values the backend returned.
  // -----------------------------------------------------------------

  it('renders chart points byte-for-byte from the backend response (AC-1.3)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN the success fixture has 3 series with 5 points each (15 total)
    // WHEN buildOption runs over the parsed QueryOutcome.success
    // THEN option.series[0].data is byte-equal to the fixture's [ts, v] tuples
    // AND option.series[0].smooth === false
    // AND option.series[0].connectNulls === false
    // AND option.series[0].sampling === undefined
    // (the comprehensive five-point NaN-bearing test lives in invariant-fidelity.test.ts;
    //  this test pins the same invariant against the multi-series fixture)
  });
});

// =============================================================================
// US-PR-06 — page chrome: backend identification
// =============================================================================

describe('Slice 01 chrome — when I open Prism on a configured deployment', () => {
  it('displays the backend label from /config.json on every render (AC-6.1, AC-6.3)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN /config.json contains backend.label="dev-local-prom"
    // WHEN the SPA loads
    // THEN the page chrome shows "backend: dev-local-prom"
    // AND the chrome remains visible after I press Run
    // AND the chrome remains visible after a successful chart paint
  });

  it('fetches /config.json exactly once per page load (AC-6.1)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN I open Prism and run a query
    // WHEN multiple QueryPanel renders happen due to React updates
    // THEN /config.json was fetched exactly once
  });
});

// =============================================================================
// URL state codec — pure function tests (ADR-0028)
// US-PR-04 AC-4.1 + AC-4.2 (within-session) anchored here as
// pure-function structural enforcement.
// KPI 4 codec property: decode(encode(state)) === state.
// =============================================================================

describe('Slice 01 URL codec — when I encode and decode the canonical URL states', () => {
  // The hand-rolled enumeration per wave-decisions.md > D9.
  const relativeOffsets: RelativeOffset[] = ['-5m', '-15m', '-1h', '-6h', '-24h'];
  const refreshIntervals: RefreshInterval[] = ['off', '5s', '10s', '30s', '1m'];
  const queries: string[] = ['', 'up', 'rate(http_requests_total[5m])'];

  const canonicalUrlStates: UrlState[] = [];
  for (const q of queries) {
    for (const offset of relativeOffsets) {
      for (const refresh of refreshIntervals) {
        canonicalUrlStates.push({
          q,
          range: { kind: 'relative', from: offset },
          refresh,
        });
      }
    }
  }

  it('roundtrips every canonical UrlState losslessly: decode(encode(state)) === state (KPI 4)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN every canonical UrlState in the enumeration
    // WHEN I encode then decode
    // THEN the decoded value equals the original (deep equality)
    //
    // for (const state of canonicalUrlStates) {
    //   const params = encode(state);
    //   const result = decode(params);
    //   expect(result.ok).toBe(true);
    //   if (result.ok) {
    //     expect(result.value).toEqual(state);
    //   }
    // }
  });

  it('encodes parameters in canonical order: q, from, to, refresh (AC-4.1)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN a UrlState with q="up", from="-15m", refresh="30s"
    // WHEN I encode
    // THEN the resulting URLSearchParams.toString() is "q=up&from=-15m&to=now&refresh=30s"
    // (canonical order, not "refresh=30s&q=up&...")
  });

  it('omits "refresh" from the URL when refresh is "off" (AC-5.1)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN a UrlState with refresh="off"
    // WHEN I encode
    // THEN the URL contains q, from, to but NO refresh parameter
  });

  it('decodes an absent "from" as the default "-15m" (default range)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN URLSearchParams("q=up")
    // WHEN I decode
    // THEN result.ok is true
    // AND result.value.range.from === "-15m"
    // (forgiving-on-input — ADR-0028 § 3)
  });

  it('decodes an absent "q" as the empty string (no query yet)', () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN URLSearchParams("")
    // WHEN I decode
    // THEN result.ok is true
    // AND result.value.q === ""
  });
});

// =============================================================================
// US-PR-04 AC-4.1 — synchronous URL writes via history.replaceState
// =============================================================================

describe('Slice 01 URL writes — when I make a state-affecting change', () => {
  beforeEach(() => {
    // Restore the URL between tests
    window.history.replaceState({}, '', '/');
  });

  it('updates the URL synchronously when I press Run with a non-empty query (AC-4.1)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN I have loaded a fresh Prism page (URL: "/")
    // WHEN I type "up" and press Run
    // THEN window.location.search becomes "?q=up&from=-15m&to=now"
    // AND history.length is unchanged (replaceState, not pushState)
  });

  it('does not pollute browser history with every keystroke (replaceState only) (AC-4.1)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN history.length is N when I land on Prism
    // WHEN I type "up" character by character and press Run
    // THEN history.length is still N (no pushState anywhere)
  });
});

// =============================================================================
// US-PR-04 AC-4.2 — fresh page load reproduces the same view
// =============================================================================

describe('Slice 01 within-session reload — when I open the same URL in a new tab', () => {
  it('hydrates the query input from the URL parameter q (AC-4.2)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN I open Prism at "/?q=up&from=-15m&to=now"
    // WHEN the page loads
    // THEN the query input contains "up"
    // AND the time-range picker shows "Last 15 min"
  });

  it('renders the chart immediately when the URL carries a non-empty query (AC-4.2)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN I open Prism at "/?q=up&from=-15m&to=now"
    // AND the backend will return the success fixture
    // WHEN the page loads
    // THEN exactly one fetch is issued
    // AND the chart paints with 3 series
  });
});

// =============================================================================
// Test seam smoke — the fetchFn injection seam works as ADR-0027 specified
// =============================================================================

describe('Slice 01 fetch seam — when I inject a fake fetchFn', () => {
  it('passes the QueryRangeContext.fetchFn through to the call (ADR-0027 § 7)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN a fakeFetch that records calls and returns the success fixture
    // WHEN I call queryRange({q:"up", range:{kind:"relative",from:"-15m"}}, {backend, fetchFn: fakeFetch})
    // THEN fakeFetch is called exactly once
    // AND the global fetch is NOT called
    // AND the returned outcome.kind === "success"
  });

  it('does not throw on transport failure — every failure becomes a QueryOutcome (ADR-0027 § 1)', async () => {
    throw new Error('UNIMPLEMENTED — Slice 01 DELIVER');
    // GIVEN a fakeFetch that rejects with TypeError
    // WHEN I call queryRange
    // THEN the function does NOT throw
    // AND the returned outcome.kind === "transport-error"
    // AND outcome.cause.kind === "network"
  });
});
