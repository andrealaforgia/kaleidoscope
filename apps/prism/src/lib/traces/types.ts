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

// Traces data-access types — foundation for the linked-view screen.
//
// Mirrors the PromQL adapter's posture (lib/promql/types.ts): the
// client is a total function returning a discriminated outcome union;
// every failure mode is a distinct arm. The wire shapes (Span,
// LogView, TraceWithLogs) match the backend's ray::Span / log-view
// JSON. All fields readonly; optionals respect exactOptionalPropertyTypes.

/** OTel span status. `code` is the closed tri-state from ray::Span. */
export interface SpanStatus {
  readonly code: 'Unset' | 'Ok' | 'Error';
  readonly message: string;
}

/**
 * A single span as serialised by the backend `ray::Span`. `trace_id`
 * and `span_id` are lowercase hex. `events` / `links` are modelled
 * loosely as arrays of unknown until the linked-view screen needs
 * their inner shape.
 */
export interface Span {
  readonly trace_id: string;
  readonly span_id: string;
  readonly parent_span_id?: string;
  readonly name: string;
  readonly kind: string;
  readonly start_time_unix_nano: number;
  readonly end_time_unix_nano: number;
  readonly status: SpanStatus;
  readonly attributes: Readonly<Record<string, string>>;
  readonly resource_attributes: Readonly<Record<string, string>>;
  readonly events: ReadonlyArray<unknown>;
  readonly links: ReadonlyArray<unknown>;
}

/** A log record as serialised by the backend, optionally correlated to a span. */
export interface LogView {
  readonly observed_time_unix_nano: number;
  readonly severity_number: number;
  readonly severity_text: string;
  readonly body: string;
  readonly attributes: Readonly<Record<string, string>>;
  readonly resource_attributes: Readonly<Record<string, string>>;
  readonly trace_id?: string;
  readonly span_id?: string;
}

/** A trace's spans AND its correlated logs, returned by /traces/with_logs. */
export interface TraceWithLogs {
  readonly trace_id: string;
  readonly spans: readonly Span[];
  readonly logs: readonly LogView[];
}

/**
 * A flat span array grouped by trace. The listing endpoint returns a
 * flat Span[]; the UI wants per-trace grouping, produced by
 * groupSpansByTrace.
 */
export interface TraceGroup {
  readonly trace_id: string;
  readonly spans: readonly Span[];
}

/** Request parameters for the failed-trace find surface. */
export interface FailedTracesRequest {
  readonly service: string;
  /** Window start, epoch seconds. */
  readonly start: number;
  /** Window end, epoch seconds. */
  readonly end: number;
}

/**
 * Transport-failure taxonomy, mirroring lib/promql TransportCause.
 * `parse-error` (malformed JSON body) is a sibling outcome arm, not a
 * transport cause — see the outcome unions below.
 */
export type TracesTransportCause =
  | { readonly kind: 'network'; readonly message: string }
  | { readonly kind: 'http-status'; readonly status: number; readonly message: string }
  | { readonly kind: 'shape'; readonly message: string }
  | { readonly kind: 'aborted' };

/**
 * Outcome of findFailedTraces. Four arms mirror QueryOutcome's posture:
 *   success         — non-empty, grouped per-trace
 *   empty           — backend returned an empty span array
 *   parse-error     — body was not valid JSON
 *   transport-error — network failure / abort / HTTP non-2xx / shape mismatch
 */
export type FailedTracesOutcome =
  | { readonly kind: 'success'; readonly traces: readonly TraceGroup[]; readonly queryMs: number }
  | { readonly kind: 'empty'; readonly queryMs: number }
  | { readonly kind: 'parse-error'; readonly message: string; readonly queryMs: number }
  | {
      readonly kind: 'transport-error';
      readonly cause: TracesTransportCause;
      readonly queryMs: number;
    };

/**
 * Outcome of getTraceWithLogs. Same four arms; `empty` means the trace
 * carried no spans.
 */
export type TraceWithLogsOutcome =
  | { readonly kind: 'success'; readonly trace: TraceWithLogs; readonly queryMs: number }
  | { readonly kind: 'empty'; readonly queryMs: number }
  | { readonly kind: 'parse-error'; readonly message: string; readonly queryMs: number }
  | {
      readonly kind: 'transport-error';
      readonly cause: TracesTransportCause;
      readonly queryMs: number;
    };

/**
 * Context for the traces client. Mirrors QueryRangeContext: a fetchFn
 * seam keeps the client testable without monkey-patching globalThis,
 * `backend` is the same-origin `/api/v1` prefix, and signal/headers are
 * forwarded to fetch.
 */
export interface TracesContext {
  /** Backend base URL, e.g. `/api/v1` (same-origin). */
  readonly backend: string;
  /** Test seam for fetch. Defaults to globalThis.fetch in production. */
  readonly fetchFn: typeof fetch;
  /** Optional abort signal honoured by the fetch call. */
  readonly signal?: AbortSignal;
  /** Outbound HTTP headers (auth, tenancy) forwarded to fetch. */
  readonly headers?: Readonly<Record<string, string>>;
}
