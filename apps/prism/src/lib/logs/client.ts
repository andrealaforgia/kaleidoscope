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

// Logs data-access client — total function, never throws.
//
// Mirrors lib/traces/client.ts: a fetchFn seam, a discriminated outcome
// union, the same transport taxonomy. One driving port:
//   findLogs → GET /logs?start=&end=&(body_contains|min_severity)
// body_contains and min_severity are MUTUALLY EXCLUSIVE (slice-01 backend
// rule). The client enforces this at its boundary: a request carrying
// both (or neither) is refused locally — fetch is never called — so the
// wire only ever carries exactly one filter.

import type {
  FindLogsOutcome,
  FindLogsRequest,
  LogView,
  LogsContext,
  LogsTransportCause,
} from './types';

// ---------------------------------------------------------------------------
// Mutual-exclusivity resolution
// ---------------------------------------------------------------------------

type ResolvedFilter =
  | { readonly ok: true; readonly param: 'body_contains' | 'min_severity'; readonly value: string }
  | { readonly ok: false; readonly message: string };

/**
 * Pick the single filter to apply, enforcing mutual exclusivity. A
 * filter counts as supplied only when its value is a non-empty string;
 * exactly one must be supplied.
 */
function resolveFilter(request: FindLogsRequest): ResolvedFilter {
  const hasBody = request.bodyContains !== undefined && request.bodyContains.length > 0;
  const hasSeverity = request.minSeverity !== undefined && request.minSeverity.length > 0;
  if (hasBody && hasSeverity) {
    return {
      ok: false,
      message: 'body_contains and min_severity are mutually exclusive, not both',
    };
  }
  if (hasBody) {
    return { ok: true, param: 'body_contains', value: request.bodyContains as string };
  }
  if (hasSeverity) {
    return { ok: true, param: 'min_severity', value: request.minSeverity as string };
  }
  return { ok: false, message: 'supply exactly one of body_contains or min_severity' };
}

function buildLogsUrl(backend: string, request: FindLogsRequest, filter: ResolvedFilter): string {
  const params = new URLSearchParams({
    start: request.start.toString(),
    end: request.end.toString(),
  });
  if (filter.ok) params.set(filter.param, filter.value);
  return `${backend}/logs?${params.toString()}`;
}

// ---------------------------------------------------------------------------
// Wire-shape narrowing (unknown → typed, no `any`)
// ---------------------------------------------------------------------------

function isStringRecord(value: unknown): value is Record<string, string> {
  if (typeof value !== 'object' || value === null) return false;
  return Object.values(value as Record<string, unknown>).every((v) => typeof v === 'string');
}

function isLogView(value: unknown): value is LogView {
  if (typeof value !== 'object' || value === null) return false;
  const o = value as Record<string, unknown>;
  return (
    typeof o['observed_time_unix_nano'] === 'number' &&
    typeof o['severity_number'] === 'number' &&
    typeof o['severity_text'] === 'string' &&
    typeof o['body'] === 'string' &&
    isStringRecord(o['attributes']) &&
    isStringRecord(o['resource_attributes'])
  );
}

function isLogViewArray(value: unknown): value is LogView[] {
  return Array.isArray(value) && value.every(isLogView);
}

// ---------------------------------------------------------------------------
// Shared fetch + parse machinery
// ---------------------------------------------------------------------------

interface FetchedBody {
  readonly json: unknown;
}

type FetchFailure =
  | { readonly kind: 'parse-error'; readonly message: string }
  | { readonly kind: 'transport-error'; readonly cause: LogsTransportCause };

type FetchResult =
  | { readonly ok: true; readonly body: FetchedBody }
  | { readonly ok: false; readonly failure: FetchFailure };

function buildInit(ctx: LogsContext): RequestInit {
  const init: RequestInit = {};
  if (ctx.signal !== undefined) init.signal = ctx.signal;
  if (ctx.headers !== undefined) init.headers = { ...ctx.headers };
  return init;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/**
 * Issue the GET and resolve the body to parsed JSON, classifying every
 * failure mode. Never throws.
 */
async function fetchJson(ctx: LogsContext, url: string): Promise<FetchResult> {
  let response: Response;
  try {
    response = await ctx.fetchFn(url, buildInit(ctx));
  } catch (err) {
    if (err instanceof Error && err.name === 'AbortError') {
      return { ok: false, failure: { kind: 'transport-error', cause: { kind: 'aborted' } } };
    }
    return {
      ok: false,
      failure: { kind: 'transport-error', cause: { kind: 'network', message: errorMessage(err) } },
    };
  }

  let body: string;
  try {
    body = await response.text();
  } catch (err) {
    return {
      ok: false,
      failure: { kind: 'transport-error', cause: { kind: 'network', message: errorMessage(err) } },
    };
  }

  if (!response.ok) {
    return {
      ok: false,
      failure: {
        kind: 'transport-error',
        cause: { kind: 'http-status', status: response.status, message: body.slice(0, 200) },
      },
    };
  }

  let json: unknown;
  try {
    json = JSON.parse(body);
  } catch (err) {
    return { ok: false, failure: { kind: 'parse-error', message: errorMessage(err) } };
  }

  return { ok: true, body: { json } };
}

// ---------------------------------------------------------------------------
// Driving port
// ---------------------------------------------------------------------------

/**
 * Find logs in a window by EITHER a body substring OR a severity floor —
 * never both. Total function: every failure mode is an outcome arm. A
 * request carrying both filters (or neither) is refused locally with a
 * transport-error/invalid-request cause, and fetch is never called.
 */
export async function findLogs(
  ctx: LogsContext,
  request: FindLogsRequest,
): Promise<FindLogsOutcome> {
  const filter = resolveFilter(request);
  if (!filter.ok) {
    return {
      kind: 'transport-error',
      cause: { kind: 'invalid-request', message: filter.message },
      queryMs: 0,
    };
  }

  const startMs = performance.now();
  const result = await fetchJson(ctx, buildLogsUrl(ctx.backend, request, filter));
  const queryMs = Math.round(performance.now() - startMs);

  if (!result.ok) {
    return { ...result.failure, queryMs };
  }

  if (!isLogViewArray(result.body.json)) {
    return {
      kind: 'transport-error',
      cause: { kind: 'shape', message: 'response was not a log array' },
      queryMs,
    };
  }

  if (result.body.json.length === 0) {
    return { kind: 'empty', queryMs };
  }

  return { kind: 'success', logs: result.body.json, queryMs };
}
