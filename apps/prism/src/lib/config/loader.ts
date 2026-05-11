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

// ADR-0030 — `/config.json` loader. Total function: returns a
// LoadConfigResult discriminated union; never throws. The App
// composition root refuses to mount on `LoadConfigResult.error`
// per the wire-then-probe-then-use posture.

import type { ConfigError, RuntimeConfig } from './types';

export type LoadConfigResult =
  | { readonly kind: 'ok'; readonly config: RuntimeConfig }
  | { readonly kind: 'error'; readonly error: ConfigError };

export interface LoadConfigContext {
  /** Test seam for fetch; defaults to globalThis.fetch in production wiring. */
  readonly fetchFn: typeof fetch;
}

function isRuntimeConfig(obj: unknown): obj is RuntimeConfig {
  if (typeof obj !== 'object' || obj === null) return false;
  const o = obj as Record<string, unknown>;
  const backend = o['backend'];
  const prism = o['prism'];
  if (typeof backend !== 'object' || backend === null) return false;
  if (typeof prism !== 'object' || prism === null) return false;
  const b = backend as Record<string, unknown>;
  const p = prism as Record<string, unknown>;
  return (
    typeof b['url'] === 'string' &&
    typeof b['label'] === 'string' &&
    typeof p['version'] === 'string'
  );
}

/**
 * Load and validate `/config.json`. Three error arms cover every
 * failure mode that prevents Prism from mounting:
 *
 *   fetch-failed  — network failure or HTTP non-200
 *   parse-failed  — non-JSON body
 *   shape-failed  — JSON missing the RuntimeConfig fields
 *
 * The App composition root reads the result kind and refuses to
 * render the QueryPanel on error, displaying a calm error UI per
 * ADR-0026 §5 and ADR-0028 §6's malformed-URL fallback pattern.
 */
export async function loadConfig(ctx: LoadConfigContext): Promise<LoadConfigResult> {
  let response: Response;
  try {
    response = await ctx.fetchFn('/config.json');
  } catch (err) {
    return {
      kind: 'error',
      error: {
        kind: 'fetch-failed',
        message: err instanceof Error ? err.message : String(err),
      },
    };
  }

  if (!response.ok) {
    return {
      kind: 'error',
      error: {
        kind: 'fetch-failed',
        message: `HTTP ${response.status} ${response.statusText}`,
      },
    };
  }

  let body: string;
  try {
    body = await response.text();
  } catch (err) {
    return {
      kind: 'error',
      error: {
        kind: 'fetch-failed',
        message: err instanceof Error ? err.message : String(err),
      },
    };
  }

  let json: unknown;
  try {
    json = JSON.parse(body);
  } catch (err) {
    return {
      kind: 'error',
      error: {
        kind: 'parse-failed',
        message: err instanceof Error ? err.message : String(err),
      },
    };
  }

  if (!isRuntimeConfig(json)) {
    return {
      kind: 'error',
      error: {
        kind: 'shape-failed',
        message: 'response missing required RuntimeConfig fields',
      },
    };
  }

  return { kind: 'ok', config: json };
}
