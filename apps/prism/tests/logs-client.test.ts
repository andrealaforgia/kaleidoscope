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

// Logs data-access client — unit tests.
//
// Foundation for the iteration-2 symptom-search screen. The SPA reaches
// the same-origin log query backend (relative /api/v1):
//   GET /api/v1/logs?start=&end=&body_regex=(?i)<escaped>  → flat LogView[]
//   GET /api/v1/logs?start=&end=&min_severity=<level>      → flat LogView[]
// The on-screen "body contains" box is CASE-INSENSITIVE: the client sends
// the user's literal text as a regex-ESCAPED, case-insensitive `body_regex`
// (a leading `(?i)` honoured by the backend's Rust `regex` crate), NOT the
// case-sensitive `body_contains`. body_regex and min_severity are MUTUALLY
// EXCLUSIVE (backend rule): the client must NEVER put both on the wire.
//
// Mirrors the traces client testing posture: a fetchFn seam, a total
// function that never throws, and a discriminated-outcome taxonomy
// (success / empty / parse-error / transport-error).

import { describe, expect, it } from 'vitest';

import { findLogs } from '../src/lib/logs/client';
import type { LogView, LogsContext } from '../src/lib/logs/types';

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

function recordingCtx(
  responder: (url: string, init?: RequestInit) => Response | Promise<Response>,
  extra: Partial<LogsContext> = {},
): { ctx: LogsContext; calls: { url: string; init: RequestInit | undefined }[] } {
  const calls: { url: string; init: RequestInit | undefined }[] = [];
  const fetchFn: typeof fetch = async (input, init) => {
    calls.push({ url: String(input), init });
    return responder(String(input), init);
  };
  return { ctx: { backend: '/api/v1', fetchFn, ...extra }, calls };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

// ===========================================================================
// body search — CASE-INSENSITIVE, regex-escaped body_regex
// ===========================================================================

describe('findLogs — case-insensitive body search via (?i) escaped body_regex', () => {
  it('builds GET /logs sending a case-insensitive (?i) escaped body_regex and NEVER body_contains', async () => {
    const logs = [
      makeLog({
        body: 'payment gateway returned 402',
        severity_text: 'ERROR',
        severity_number: 17,
      }),
      makeLog({ body: 'payment retried', severity_text: 'WARN', severity_number: 13 }),
    ];
    const { ctx, calls } = recordingCtx(() => jsonResponse(logs));

    const outcome = await findLogs(ctx, { start: 1000, end: 2000, bodyContains: 'payment' });

    expect(calls).toHaveLength(1);
    const issued = new URL(calls[0]!.url, 'http://x');
    expect(issued.pathname).toBe('/api/v1/logs');
    expect(issued.searchParams.get('start')).toBe('1000');
    expect(issued.searchParams.get('end')).toBe('2000');
    // The wire carries a case-insensitive regex, NOT the case-sensitive
    // body_contains (which is never on the wire).
    expect(issued.searchParams.get('body_regex')).toBe('(?i)payment');
    expect(issued.searchParams.has('body_contains')).toBe(false);
    // The mutually-exclusive sibling is never on the wire either.
    expect(issued.searchParams.has('min_severity')).toBe(false);

    expect(outcome.kind).toBe('success');
    if (outcome.kind === 'success') {
      expect(outcome.logs).toHaveLength(2);
      expect(outcome.logs[0]!.body).toBe('payment gateway returned 402');
    }
  });

  it('a capital "Declined" yields (?i)Declined, which case-insensitively matches "card declined"', async () => {
    const { ctx, calls } = recordingCtx(() => jsonResponse([makeLog()]));

    await findLogs(ctx, { start: 1, end: 2, bodyContains: 'Declined' });

    const value = new URL(calls[0]!.url, 'http://x').searchParams.get('body_regex');
    // The exact param value: (?i) + the (here metachar-free) escaped text.
    expect(value).toBe('(?i)Declined');
    // PROOF it is genuinely case-insensitive: applied as the backend's Rust
    // `regex` crate would (the (?i) flag → JS `i` flag), the capital query
    // still matches the lowercase body.
    const body = 'checkout failed: card declined';
    const pattern = value!.replace(/^\(\?i\)/, '');
    expect(new RegExp(pattern, 'i').test(body)).toBe(true);
    // And without case-folding it would NOT — so (?i) is load-bearing.
    expect(new RegExp(pattern).test(body)).toBe(false);
  });

  it.each([
    ['GET /api/v1/x?y', '(?i)GET /api/v1/x\\?y'],
    ['a.b*c', '(?i)a\\.b\\*c'],
    ['(1+1)', '(?i)\\(1\\+1\\)'],
    ['cost is $5', '(?i)cost is \\$5'],
  ])('escapes regex metacharacters so %j is matched literally', async (query, expected) => {
    const { ctx, calls } = recordingCtx(() => jsonResponse([makeLog()]));

    await findLogs(ctx, { start: 1, end: 2, bodyContains: query });

    const value = new URL(calls[0]!.url, 'http://x').searchParams.get('body_regex');
    expect(value).toBe(expected);
    // The escaped pattern matches the user's literal text verbatim (and
    // case-insensitively), proving the metachars were neutralised.
    const pattern = value!.replace(/^\(\?i\)/, '');
    expect(new RegExp(pattern, 'i').test(query)).toBe(true);
  });

  it('url-encodes a body_regex value carrying spaces', async () => {
    const { ctx, calls } = recordingCtx(() => jsonResponse([makeLog()]));
    await findLogs(ctx, { start: 1, end: 2, bodyContains: 'card declined' });
    const issued = new URL(calls[0]!.url, 'http://x');
    expect(issued.searchParams.get('body_regex')).toBe('(?i)card declined');
    expect(calls[0]!.url).toContain('body_regex=%28%3Fi%29card+declined');
  });
});

// ===========================================================================
// min-severity search
// ===========================================================================

describe('findLogs — min-severity search', () => {
  it('builds GET /logs with min_severity (the floor) and never body_contains, returning the floor set', async () => {
    const floorSet = [
      makeLog({ body: 'disk almost full', severity_text: 'WARN', severity_number: 13 }),
      makeLog({ body: 'card declined', severity_text: 'ERROR', severity_number: 17 }),
    ];
    const { ctx, calls } = recordingCtx(() => jsonResponse(floorSet));

    const outcome = await findLogs(ctx, { start: 5, end: 6, minSeverity: 'WARN' });

    const issued = new URL(calls[0]!.url, 'http://x');
    expect(issued.pathname).toBe('/api/v1/logs');
    expect(issued.searchParams.get('min_severity')).toBe('WARN');
    // The mutually-exclusive sibling is never on the wire.
    expect(issued.searchParams.has('body_contains')).toBe(false);

    expect(outcome.kind).toBe('success');
    if (outcome.kind === 'success') {
      expect(outcome.logs.map((l) => l.severity_text)).toEqual(['WARN', 'ERROR']);
    }
  });
});

// ===========================================================================
// MUTUAL EXCLUSIVITY — the client never sends both filters
// ===========================================================================

describe('findLogs — body_contains and min_severity are mutually exclusive at the client boundary', () => {
  it('refuses to issue the request when BOTH filters are supplied, and never calls fetch', async () => {
    const { ctx, calls } = recordingCtx(() => jsonResponse([makeLog()]));

    const outcome = await findLogs(ctx, {
      start: 1,
      end: 2,
      bodyContains: 'payment',
      minSeverity: 'WARN',
    });

    // The request never leaves the client — the wire never carries both.
    expect(calls).toHaveLength(0);
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('invalid-request');
      if (outcome.cause.kind === 'invalid-request') {
        expect(outcome.cause.message).toMatch(/mutually exclusive|not both/i);
      }
    }
  });

  it('refuses to issue the request when NEITHER filter is supplied, and never calls fetch', async () => {
    const { ctx, calls } = recordingCtx(() => jsonResponse([makeLog()]));
    const outcome = await findLogs(ctx, { start: 1, end: 2 });
    expect(calls).toHaveLength(0);
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('invalid-request');
    }
  });
});

// ===========================================================================
// OUTCOME ARMS — empty / parse-error / transport-error
// ===========================================================================

describe('findLogs — outcome arms', () => {
  it('returns the empty arm when the backend returns an empty log array', async () => {
    const { ctx } = recordingCtx(() => jsonResponse([]));
    const outcome = await findLogs(ctx, { start: 1, end: 2, bodyContains: 'nothing' });
    expect(outcome.kind).toBe('empty');
  });

  it('returns parse-error when the body is not valid JSON', async () => {
    const { ctx } = recordingCtx(() => new Response('<<not json>>', { status: 200 }));
    const outcome = await findLogs(ctx, { start: 1, end: 2, bodyContains: 'x' });
    expect(outcome.kind).toBe('parse-error');
    if (outcome.kind === 'parse-error') {
      expect(outcome.message.length).toBeGreaterThan(0);
    }
  });

  it('returns transport-error (network) when fetch throws', async () => {
    const { ctx } = recordingCtx(() => {
      throw new Error('connection refused');
    });
    const outcome = await findLogs(ctx, { start: 1, end: 2, minSeverity: 'ERROR' });
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('network');
    }
  });

  it('returns transport-error (aborted) when the fetch is aborted', async () => {
    const { ctx } = recordingCtx(() => {
      const err = new Error('aborted');
      err.name = 'AbortError';
      throw err;
    });
    const outcome = await findLogs(ctx, { start: 1, end: 2, bodyContains: 'x' });
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('aborted');
    }
  });

  it('returns transport-error (http-status) on a non-2xx response', async () => {
    const { ctx } = recordingCtx(() => new Response('upstream exploded', { status: 503 }));
    const outcome = await findLogs(ctx, { start: 1, end: 2, bodyContains: 'x' });
    expect(outcome.kind).toBe('transport-error');
    if (outcome.kind === 'transport-error') {
      expect(outcome.cause.kind).toBe('http-status');
      if (outcome.cause.kind === 'http-status') {
        expect(outcome.cause.status).toBe(503);
      }
    }
  });

  it('returns transport-error (shape) when valid JSON is not a log array', async () => {
    const { ctx } = recordingCtx(() => jsonResponse({ not: 'an array' }));
    const outcome = await findLogs(ctx, { start: 1, end: 2, bodyContains: 'x' });
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
    await findLogs(ctx, { start: 1, end: 2, bodyContains: 'x' });
    expect(calls[0]!.init?.signal).toBe(controller.signal);
    expect(calls[0]!.init?.headers).toMatchObject({ authorization: 'Bearer t0ken' });
  });
});
