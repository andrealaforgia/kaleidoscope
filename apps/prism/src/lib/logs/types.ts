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

// Logs data-access types — foundation for the symptom-search screen.
//
// Mirrors lib/traces/types.ts: the client is a total function returning
// a discriminated outcome union; every failure mode is a distinct arm.
// The wire shape (LogView) matches the backend's log-view JSON. All
// fields readonly; optionals respect exactOptionalPropertyTypes.

/**
 * A log record as serialised by the backend, optionally correlated to a
 * span via trace_id / span_id. Structurally identical to the trace
 * view's correlated log, but owned here so the logs module stands alone.
 */
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

/**
 * Request parameters for a log symptom search. `bodyContains` (a
 * CASE-INSENSITIVE literal body substring — the client sends it as a
 * regex-escaped, case-insensitive `body_regex`, so "Declined" still finds
 * "card declined") and `minSeverity` (an OTel severity floor:
 * TRACE / DEBUG / INFO / WARN / ERROR / FATAL) are MUTUALLY EXCLUSIVE —
 * the backend rule. The client refuses any request carrying both (or
 * neither); it never puts both on the wire.
 */
export interface FindLogsRequest {
  /** Window start, epoch seconds. */
  readonly start: number;
  /** Window end, epoch seconds. */
  readonly end: number;
  /** Case-insensitive literal body substring. Exclusive with `minSeverity`. */
  readonly bodyContains?: string;
  /** OTel severity floor name. Exclusive with `bodyContains`. */
  readonly minSeverity?: string;
}

/**
 * Transport-failure taxonomy, mirroring lib/traces TracesTransportCause
 * with one addition: `invalid-request` is a CLIENT-side refusal — the
 * request was rejected locally (e.g. both filters supplied) and never
 * reached the backend. `parse-error` (malformed JSON body) is a sibling
 * outcome arm, not a transport cause.
 */
export type LogsTransportCause =
  | { readonly kind: 'network'; readonly message: string }
  | { readonly kind: 'http-status'; readonly status: number; readonly message: string }
  | { readonly kind: 'shape'; readonly message: string }
  | { readonly kind: 'aborted' }
  | { readonly kind: 'invalid-request'; readonly message: string };

/**
 * Outcome of findLogs. Four arms mirror the traces client's posture:
 *   success         — non-empty log array
 *   empty           — backend returned an empty log array
 *   parse-error     — body was not valid JSON
 *   transport-error — network / abort / HTTP non-2xx / shape / invalid-request
 */
export type FindLogsOutcome =
  | { readonly kind: 'success'; readonly logs: readonly LogView[]; readonly queryMs: number }
  | { readonly kind: 'empty'; readonly queryMs: number }
  | { readonly kind: 'parse-error'; readonly message: string; readonly queryMs: number }
  | {
      readonly kind: 'transport-error';
      readonly cause: LogsTransportCause;
      readonly queryMs: number;
    };

/**
 * Context for the logs client. Mirrors TracesContext: a fetchFn seam
 * keeps the client testable without monkey-patching globalThis,
 * `backend` is the same-origin `/api/v1` prefix, and signal/headers are
 * forwarded to fetch.
 */
export interface LogsContext {
  /** Backend base URL, e.g. `/api/v1` (same-origin). */
  readonly backend: string;
  /** Test seam for fetch. Defaults to globalThis.fetch in production. */
  readonly fetchFn: typeof fetch;
  /** Optional abort signal honoured by the fetch call. */
  readonly signal?: AbortSignal;
  /** Outbound HTTP headers (auth, tenancy) forwarded to fetch. */
  readonly headers?: Readonly<Record<string, string>>;
}
