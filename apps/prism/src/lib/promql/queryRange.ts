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

// ADR-0027 §1 — Total-function queryRange. Never throws; every
// failure mode is a QueryOutcome arm.

import type {
  LabelSet,
  QueryOutcome,
  QueryRangeContext,
  QueryRangeRequest,
  Series,
  TransportCause,
} from './types';
import type { TimeRange } from '../url-state/types';

const STEP_SECONDS = 15;

/** Convert a relative offset like "-15m" to seconds. */
function relativeOffsetToSeconds(offset: string): number {
  const match = /^-(\d+)(m|h|d)$/.exec(offset);
  if (match === null) {
    return 900; // safe fallback: 15m
  }
  const n = Number.parseInt(match[1]!, 10);
  const unit = match[2]!;
  switch (unit) {
    case 'm':
      return n * 60;
    case 'h':
      return n * 3600;
    case 'd':
      return n * 86_400;
    default:
      return 900;
  }
}

/** Resolve a TimeRange into Prometheus query_range start/end seconds. */
function resolveRange(range: TimeRange): { start: number; end: number } {
  if (range.kind === 'absolute') {
    return { start: range.from.getTime() / 1000, end: range.to.getTime() / 1000 };
  }
  const nowSec = Date.now() / 1000;
  const fromSec = nowSec - relativeOffsetToSeconds(range.from);
  return { start: fromSec, end: nowSec };
}

/** Build the query_range URL against the configured backend prefix. */
function buildUrl(backend: string, request: QueryRangeRequest): string {
  const { start, end } = resolveRange(request.range);
  const params = new URLSearchParams({
    query: request.q,
    start: start.toString(),
    end: end.toString(),
    step: `${STEP_SECONDS}s`,
  });
  return `${backend}/query_range?${params.toString()}`;
}

interface PromValuePair {
  0: number;
  1: string;
}
interface PromMatrixEntry {
  metric: Record<string, string>;
  values: PromValuePair[];
}
interface PromSuccess {
  status: 'success';
  data: { resultType: 'matrix'; result: PromMatrixEntry[] };
}
interface PromError {
  status: 'error';
  error: string;
}

function parseValue(raw: string): number {
  if (raw === 'NaN') return Number.NaN;
  return Number.parseFloat(raw);
}

function parseSeries(result: PromMatrixEntry[]): Series[] {
  return result.map((entry) => ({
    labels: entry.metric as LabelSet,
    points: entry.values.map((v) => [v[0] * 1000, parseValue(v[1])] as readonly [number, number]),
  }));
}

/** Type guard: distinguish Prometheus success shape from the error shape. */
function isPromSuccess(obj: unknown): obj is PromSuccess {
  if (typeof obj !== 'object' || obj === null) return false;
  const o = obj as Record<string, unknown>;
  if (o['status'] !== 'success') return false;
  const data = o['data'];
  if (typeof data !== 'object' || data === null) return false;
  return Array.isArray((data as Record<string, unknown>)['result']);
}

function isPromError(obj: unknown): obj is PromError {
  if (typeof obj !== 'object' || obj === null) return false;
  const o = obj as Record<string, unknown>;
  return o['status'] === 'error' && typeof o['error'] === 'string';
}

/**
 * Total-function PromQL `query_range` call. Every failure mode is
 * encoded as a QueryOutcome arm; this function never throws.
 *
 * Wire shape: ADR-0027 §3-4. Five outcome arms:
 *   success         — status=success + data.result non-empty
 *   empty           — status=success + data.result empty
 *   parse-error     — status>=400 + status:error JSON body
 *   transport-error — network failure / HTTP non-200 (not parse-error)
 *                     / invalid JSON / shape mismatch / abort
 *   config-error    — reserved for the caller's loadConfig integration;
 *                     queryRange itself never emits this arm.
 */
export async function queryRange(
  request: QueryRangeRequest,
  ctx: QueryRangeContext,
): Promise<QueryOutcome> {
  const startMs = performance.now();
  const url = buildUrl(ctx.backend, request);

  let response: Response;
  try {
    response = await ctx.fetchFn(url, ctx.signal !== undefined ? { signal: ctx.signal } : {});
  } catch (err) {
    const queryMs = Math.round(performance.now() - startMs);
    const cause: TransportCause =
      err instanceof Error && err.name === 'AbortError'
        ? { kind: 'aborted' }
        : {
            kind: 'network',
            message: err instanceof Error ? err.message : String(err),
          };
    return { kind: 'transport-error', cause, queryMs };
  }

  let body: string;
  try {
    body = await response.text();
  } catch (err) {
    const queryMs = Math.round(performance.now() - startMs);
    return {
      kind: 'transport-error',
      cause: {
        kind: 'network',
        message: err instanceof Error ? err.message : String(err),
      },
      queryMs,
    };
  }

  let json: unknown;
  try {
    json = JSON.parse(body);
  } catch (err) {
    const queryMs = Math.round(performance.now() - startMs);
    return {
      kind: 'transport-error',
      cause: {
        kind: 'invalid-json',
        message: err instanceof Error ? err.message : String(err),
      },
      queryMs,
    };
  }

  const queryMs = Math.round(performance.now() - startMs);

  if (!response.ok) {
    if (isPromError(json)) {
      return { kind: 'parse-error', backendError: json.error, queryMs };
    }
    return {
      kind: 'transport-error',
      cause: {
        kind: 'http-status',
        status: response.status,
        message: body.slice(0, 200),
      },
      queryMs,
    };
  }

  if (isPromError(json)) {
    // status:error inside a 200 response — treat as parse-error too.
    return { kind: 'parse-error', backendError: json.error, queryMs };
  }

  if (!isPromSuccess(json)) {
    return {
      kind: 'transport-error',
      cause: { kind: 'shape', message: 'response missing data.result' },
      queryMs,
    };
  }

  if (json.data.result.length === 0) {
    return { kind: 'empty', queryMs };
  }

  return { kind: 'success', series: parseSeries(json.data.result), queryMs };
}
