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

// Traces data-access client — unit tests.
//
// Foundation for the upcoming "linked view" screen. The SPA reaches two
// backend routes on its OWN origin (same-origin, relative /api/v1):
//   GET /api/v1/traces?service=&start=&end=&error=true  → flat Span[]
//   GET /api/v1/traces/with_logs?trace_id=              → TraceWithLogs
//
// These tests mirror the queryRange testing posture: a fetchFn seam, a
// total-function client that never throws, and a discriminated-outcome
// taxonomy (success / empty / parse-error / transport-error). Pure
// grouping helper is exercised directly through its public signature.

import { describe, expect, it } from 'vitest';

import { findFailedTraces, getTraceWithLogs, groupSpansByTrace } from '../src/lib/traces/client';
import type { LogView, Span, TracesContext } from '../src/lib/traces/types';

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

function makeSpan(overrides: Partial<Span> & Pick<Span, 'trace_id' | 'span_id'>): Span {
  return {
    name: 'GET /checkout',
    kind: 'Server',
    start_time_unix_nano: 1_700_000_000_000_000_000,
    end_time_unix_nano: 1_700_000_000_100_000_000,
    status: { code: 'Unset', message: '' },
    attributes: {},
    resource_attributes: {},
    events: [],
    links: [],
    ...overrides,
  };
}

function makeLog(overrides: Partial<LogView> = {}): LogView {
  return {
    observed_time_unix_nano: 1_700_000_000_050_000_000,
    severity_number: 17,
    severity_text: 'ERROR',
    body: 'checkout failed',
    attributes: {},
    resource_attributes: {},
    ...overrides,
  };
}

/** Build a TracesContext whose fetchFn records calls and returns a canned Response. */
function recordingCtx(
  responder: (url: string, init?: RequestInit) => Response | Promise<Response>,
  extra: Partial<TracesContext> = {},
): { ctx: TracesContext; calls: { url: string; init: RequestInit | undefined }[] } {
  const calls: { url: string; init: RequestInit | undefined }[] = [];
  const fetchFn: typeof fetch = async (input, init) => {
    calls.push({ url: String(input), init });
    return responder(String(input), init);
  };
  return {
    ctx: { backend: '/api/v1', fetchFn, ...extra },
    calls,
  };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

// ===========================================================================
// findFailedTraces
// ===========================================================================

describe('findFailedTraces — the failed-trace find surface', () => {
  it('builds GET /traces with service, start, end and error=true, returning grouped traces on success', async () => {
    const spans = [
      makeSpan({ trace_id: 'aaaa', span_id: '01', name: 'root' }),
      makeSpan({
        trace_id: 'aaaa',
        span_id: '02',
        parent_span_id: '01',
        status: { code: 'Error', message: 'boom' },
      }),
      makeSpan({ trace_id: 'bbbb', span_id: '03' }),
    ];
    const { ctx, calls } = recordingCtx(() => jsonResponse(spans));

    const outcome = await findFailedTraces(ctx, {
      service: 'checkout svc',
      start: 1000,
      end: 2000,
    });

    // URL contract
    expect(calls).toHaveLength(1);
    const issued = new URL(calls[0]!.url, 'http://x');
    expect(issued.pathname).toBe('/api/v1/traces');
    expect(issued.searchParams.get('service')).toBe('checkout svc');
    expect(issued.searchParams.get('start')).toBe('1000');
    expect(issued.searchParams.get('end')).toBe('2000');
    expect(issued.searchParams.get('error')).toBe('true');
    // service value with a space must be URL-encoded on the wire
    expect(calls[0]!.url).toContain('service=checkout+svc');

    // Outcome: success with spans grouped into two traces
    expect(outcome.kind).toBe('success');
    if (outcome.kind === 'success') {
      expect(outcome.traces).toHaveLength(2);
      expect(outcome.traces[0]!.trace_id).toBe('aaaa');
      expect(outcome.traces[0]!.spans).toHaveLength(2);
      expect(outcome.traces[1]!.trace_id).toBe('bbbb');
      expect(outcome.traces[1]!.spans).toHaveLength(1);
    }
  });

  it('returns the empty arm when the backend returns an empty span array', async () => {
    const { ctx } = recordingCtx(() => jsonResponse([]));
    const outcome = await findFailedTraces(ctx, { service: 'svc', start: 1, end: 2 });
    expect(outcome.kind).toBe('empty');
  });

  it('returns parse-error when the body is not valid JSON', async () => {
    const { ctx } = recordingCtx(() => new Response('<<not json>>', { status: 200 }));
    const outcome = await findFailedTraces(ctx, { service: 'svc', start: 1, end: 2 });
    expect(outcome.kind).toBe('parse-error');
    if (outcome.kind === 'parse-error') {
      expect(outcome.message.length).toBeGreaterThan(0);
    }
  });

  it('returns transport-error (network) when fetch throws', async () => {
    const { ctx } = recordingCtx(() => {
      throw new Error('connection refused');
    });
    const outcome = await findFailedTraces(ctx, { service: 'svc', start: 1, end: 2 });
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('network');
      if (outcome.cause.kind === 'network') {
        expect(outcome.cause.message).toContain('connection refused');
      }
    }
  });

  it('returns transport-error (aborted) when the fetch is aborted', async () => {
    const { ctx } = recordingCtx(() => {
      const err = new Error('aborted');
      err.name = 'AbortError';
      throw err;
    });
    const outcome = await findFailedTraces(ctx, { service: 'svc', start: 1, end: 2 });
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('aborted');
    }
  });

  it('returns transport-error (http-status) on a non-2xx response', async () => {
    const { ctx } = recordingCtx(() => new Response('upstream exploded', { status: 503 }));
    const outcome = await findFailedTraces(ctx, { service: 'svc', start: 1, end: 2 });
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('http-status');
      if (outcome.cause.kind === 'http-status') {
        expect(outcome.cause.status).toBe(503);
      }
    }
  });

  it('returns transport-error (shape) when valid JSON is not a span array', async () => {
    const { ctx } = recordingCtx(() => jsonResponse({ not: 'an array' }));
    const outcome = await findFailedTraces(ctx, { service: 'svc', start: 1, end: 2 });
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('shape');
    }
  });

  it('forwards the AbortSignal and headers to fetch', async () => {
    const controller = new AbortController();
    const { ctx, calls } = recordingCtx(() => jsonResponse([]), {
      signal: controller.signal,
      headers: { authorization: 'Bearer t0ken' },
    });
    await findFailedTraces(ctx, { service: 'svc', start: 1, end: 2 });
    expect(calls[0]!.init?.signal).toBe(controller.signal);
    expect(calls[0]!.init?.headers).toMatchObject({ authorization: 'Bearer t0ken' });
  });
});

// ===========================================================================
// getTraceWithLogs
// ===========================================================================

describe('getTraceWithLogs — a trace plus its correlated logs', () => {
  it('builds GET /traces/with_logs with the url-encoded trace_id and returns the payload on success', async () => {
    const payload = {
      trace_id: 'deadbeef',
      spans: [makeSpan({ trace_id: 'deadbeef', span_id: '01' })],
      logs: [makeLog({ trace_id: 'deadbeef', span_id: '01' })],
    };
    const { ctx, calls } = recordingCtx(() => jsonResponse(payload));

    const outcome = await getTraceWithLogs(ctx, 'dead beef');

    expect(calls).toHaveLength(1);
    const issued = new URL(calls[0]!.url, 'http://x');
    expect(issued.pathname).toBe('/api/v1/traces/with_logs');
    expect(issued.searchParams.get('trace_id')).toBe('dead beef');
    expect(calls[0]!.url).toContain('trace_id=dead+beef');

    expect(outcome.kind).toBe('success');
    if (outcome.kind === 'success') {
      expect(outcome.trace.trace_id).toBe('deadbeef');
      expect(outcome.trace.spans).toHaveLength(1);
      expect(outcome.trace.logs).toHaveLength(1);
      expect(outcome.trace.logs[0]!.body).toBe('checkout failed');
    }
  });

  it('returns the empty arm when the trace has no spans', async () => {
    const { ctx } = recordingCtx(() => jsonResponse({ trace_id: 'deadbeef', spans: [], logs: [] }));
    const outcome = await getTraceWithLogs(ctx, 'deadbeef');
    expect(outcome.kind).toBe('empty');
  });

  it('returns parse-error when the body is not valid JSON', async () => {
    const { ctx } = recordingCtx(() => new Response('nope', { status: 200 }));
    const outcome = await getTraceWithLogs(ctx, 'deadbeef');
    expect(outcome.kind).toBe('parse-error');
  });

  it('returns transport-error (network) when fetch throws', async () => {
    const { ctx } = recordingCtx(() => {
      throw new Error('dns failure');
    });
    const outcome = await getTraceWithLogs(ctx, 'deadbeef');
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('network');
    }
  });

  it('returns transport-error (shape) when valid JSON is missing the TraceWithLogs fields', async () => {
    const { ctx } = recordingCtx(() => jsonResponse({ trace_id: 'x' }));
    const outcome = await getTraceWithLogs(ctx, 'deadbeef');
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('shape');
    }
  });
});

// ===========================================================================
// groupSpansByTrace — pure helper
// ===========================================================================

describe('groupSpansByTrace — flat Span[] → per-trace groups', () => {
  it('returns an empty array for no spans', () => {
    expect(groupSpansByTrace([])).toEqual([]);
  });

  it('groups multiple spans per trace, preserving first-seen trace order and span order', () => {
    const spans = [
      makeSpan({ trace_id: 't1', span_id: 'a' }),
      makeSpan({ trace_id: 't2', span_id: 'b' }),
      makeSpan({ trace_id: 't1', span_id: 'c' }),
      makeSpan({ trace_id: 't1', span_id: 'd' }),
    ];
    const groups = groupSpansByTrace(spans);
    expect(groups.map((g) => g.trace_id)).toEqual(['t1', 't2']);
    expect(groups[0]!.spans.map((s) => s.span_id)).toEqual(['a', 'c', 'd']);
    expect(groups[1]!.spans.map((s) => s.span_id)).toEqual(['b']);
  });

  it('isolates a single error trace among healthy ones', () => {
    const spans = [
      makeSpan({ trace_id: 'healthy1', span_id: '1' }),
      makeSpan({
        trace_id: 'failed',
        span_id: '2',
        status: { code: 'Error', message: 'payment declined' },
      }),
      makeSpan({ trace_id: 'healthy2', span_id: '3' }),
    ];
    const groups = groupSpansByTrace(spans);
    expect(groups).toHaveLength(3);
    const failed = groups.find((g) => g.trace_id === 'failed');
    expect(failed).toBeDefined();
    expect(failed!.spans[0]!.status.code).toBe('Error');
  });
});
