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

// Traces data-access client — total functions, never throw.
//
// Mirrors lib/promql/queryRange.ts: a fetchFn seam, a discriminated
// outcome union, the same transport taxonomy. Two driving ports:
//   findFailedTraces  → GET /traces?service=&start=&end=&error=true
//   getTraceWithLogs  → GET /traces/with_logs?trace_id=
// plus a pure helper, groupSpansByTrace, that the listing UI uses to
// turn the flat Span[] into per-trace groups.

import type {
  FailedTracesOutcome,
  FailedTracesRequest,
  LogView,
  Span,
  SpanStatus,
  TraceGroup,
  TraceWithLogs,
  TraceWithLogsOutcome,
  TracesContext,
  TracesTransportCause,
} from './types';

// ---------------------------------------------------------------------------
// URL building
// ---------------------------------------------------------------------------

/**
 * Build the trace find URL. `errorOnly` appends `error=true` so the
 * backend returns only traces with at least one Error-status span;
 * omitting it lists every trace for the service in the window.
 *
 * The attribute filter (`attr_key` + `attr_value`) is appended ONLY when
 * BOTH are non-empty — both-or-neither, structurally. A request carrying
 * exactly one therefore never emits a lone param the backend would 400 on.
 * URLSearchParams handles encoding (a dotted key like `customer.id` and a
 * value with reserved characters both go out percent-safe). Composes with
 * `error=true`.
 */
function buildTracesUrl(backend: string, request: FailedTracesRequest, errorOnly: boolean): string {
  const params = new URLSearchParams({
    service: request.service,
    start: request.start.toString(),
    end: request.end.toString(),
  });
  if (errorOnly) params.set('error', 'true');
  const attrKey = request.attrKey ?? '';
  const attrValue = request.attrValue ?? '';
  if (attrKey.length > 0 && attrValue.length > 0) {
    params.set('attr_key', attrKey);
    params.set('attr_value', attrValue);
  }
  return `${backend}/traces?${params.toString()}`;
}

/** Build the trace-with-logs URL for a single trace id. */
function buildTraceWithLogsUrl(backend: string, traceId: string): string {
  const params = new URLSearchParams({ trace_id: traceId });
  return `${backend}/traces/with_logs?${params.toString()}`;
}

// ---------------------------------------------------------------------------
// Wire-shape narrowing (unknown → typed, no `any`)
// ---------------------------------------------------------------------------

function isStringRecord(value: unknown): value is Record<string, string> {
  if (typeof value !== 'object' || value === null) return false;
  return Object.values(value as Record<string, unknown>).every((v) => typeof v === 'string');
}

function isSpanStatus(value: unknown): value is SpanStatus {
  if (typeof value !== 'object' || value === null) return false;
  const o = value as Record<string, unknown>;
  return (
    (o['code'] === 'Unset' || o['code'] === 'Ok' || o['code'] === 'Error') &&
    typeof o['message'] === 'string'
  );
}

function isSpan(value: unknown): value is Span {
  if (typeof value !== 'object' || value === null) return false;
  const o = value as Record<string, unknown>;
  return (
    typeof o['trace_id'] === 'string' &&
    typeof o['span_id'] === 'string' &&
    typeof o['name'] === 'string' &&
    typeof o['kind'] === 'string' &&
    typeof o['start_time_unix_nano'] === 'number' &&
    typeof o['end_time_unix_nano'] === 'number' &&
    isSpanStatus(o['status']) &&
    isStringRecord(o['attributes']) &&
    isStringRecord(o['resource_attributes']) &&
    Array.isArray(o['events']) &&
    Array.isArray(o['links'])
  );
}

function isSpanArray(value: unknown): value is Span[] {
  return Array.isArray(value) && value.every(isSpan);
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

function isTraceWithLogs(value: unknown): value is TraceWithLogs {
  if (typeof value !== 'object' || value === null) return false;
  const o = value as Record<string, unknown>;
  return (
    typeof o['trace_id'] === 'string' &&
    isSpanArray(o['spans']) &&
    Array.isArray(o['logs']) &&
    o['logs'].every(isLogView)
  );
}

// ---------------------------------------------------------------------------
// Pure grouping helper
// ---------------------------------------------------------------------------

/**
 * Group a flat span array into per-trace groups. First-seen trace order
 * is preserved; within each group span order is preserved. Pure: no I/O.
 */
export function groupSpansByTrace(spans: readonly Span[]): readonly TraceGroup[] {
  const order: string[] = [];
  const buckets = new Map<string, Span[]>();
  for (const span of spans) {
    const existing = buckets.get(span.trace_id);
    if (existing === undefined) {
      order.push(span.trace_id);
      buckets.set(span.trace_id, [span]);
      continue;
    }
    existing.push(span);
  }
  return order.map((traceId) => ({ trace_id: traceId, spans: buckets.get(traceId) ?? [] }));
}

// ---------------------------------------------------------------------------
// Shared fetch + parse machinery
// ---------------------------------------------------------------------------

interface FetchedBody {
  readonly json: unknown;
}

type FetchFailure =
  | { readonly kind: 'parse-error'; readonly message: string }
  | { readonly kind: 'transport-error'; readonly cause: TracesTransportCause };

type FetchResult =
  | { readonly ok: true; readonly body: FetchedBody }
  | { readonly ok: false; readonly failure: FetchFailure };

function buildInit(ctx: TracesContext): RequestInit {
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
async function fetchJson(ctx: TracesContext, url: string): Promise<FetchResult> {
  let response: Response;
  try {
    response = await ctx.fetchFn(url, buildInit(ctx));
  } catch (err) {
    if (err instanceof Error && err.name === 'AbortError') {
      return { ok: false, failure: { kind: 'transport-error', cause: { kind: 'aborted' } } };
    }
    return {
      ok: false,
      failure: {
        kind: 'transport-error',
        cause: { kind: 'network', message: errorMessage(err) },
      },
    };
  }

  let body: string;
  try {
    body = await response.text();
  } catch (err) {
    return {
      ok: false,
      failure: {
        kind: 'transport-error',
        cause: { kind: 'network', message: errorMessage(err) },
      },
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
// Driving ports
// ---------------------------------------------------------------------------

/**
 * Shared find core for the listing surface. `errorOnly` selects between
 * the failed-only and the all-traces query. Total: every failure mode is
 * an outcome arm.
 */
async function fetchTraceList(
  ctx: TracesContext,
  request: FailedTracesRequest,
  errorOnly: boolean,
): Promise<FailedTracesOutcome> {
  const startMs = performance.now();
  const result = await fetchJson(ctx, buildTracesUrl(ctx.backend, request, errorOnly));
  const queryMs = Math.round(performance.now() - startMs);

  if (!result.ok) {
    return { ...result.failure, queryMs };
  }

  if (!isSpanArray(result.body.json)) {
    return {
      kind: 'transport-error',
      cause: { kind: 'shape', message: 'response was not a span array' },
      queryMs,
    };
  }

  if (result.body.json.length === 0) {
    return { kind: 'empty', queryMs };
  }

  return { kind: 'success', traces: groupSpansByTrace(result.body.json), queryMs };
}

/**
 * Find traces that have at least one Error-status span for a service in
 * a window (error=true). Total function: every failure mode is an
 * outcome arm.
 */
export async function findFailedTraces(
  ctx: TracesContext,
  request: FailedTracesRequest,
): Promise<FailedTracesOutcome> {
  return fetchTraceList(ctx, request, true);
}

/**
 * List every trace for a service in a window (no error filter). The flat
 * span array is grouped per-trace. Total function: every failure mode is
 * an outcome arm.
 */
export async function findTraces(
  ctx: TracesContext,
  request: FailedTracesRequest,
): Promise<FailedTracesOutcome> {
  return fetchTraceList(ctx, request, false);
}

/**
 * Fetch a single trace's spans together with its correlated logs in one
 * call. Total function: every failure mode is an outcome arm.
 */
export async function getTraceWithLogs(
  ctx: TracesContext,
  traceId: string,
): Promise<TraceWithLogsOutcome> {
  const startMs = performance.now();
  const result = await fetchJson(ctx, buildTraceWithLogsUrl(ctx.backend, traceId));
  const queryMs = Math.round(performance.now() - startMs);

  if (!result.ok) {
    return { ...result.failure, queryMs };
  }

  if (!isTraceWithLogs(result.body.json)) {
    return {
      kind: 'transport-error',
      cause: { kind: 'shape', message: 'response missing trace_id / spans / logs' },
      queryMs,
    };
  }

  if (result.body.json.spans.length === 0) {
    return { kind: 'empty', queryMs };
  }

  return { kind: 'success', trace: result.body.json, queryMs };
}
