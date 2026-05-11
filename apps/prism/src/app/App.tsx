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

// ADR-0026 §5 — Composition root. Loads /config.json on mount;
// refuses to render QueryPanel on ConfigError per the
// wire-then-probe-then-use posture.

import { useEffect, useState, type JSX } from 'react';

import { loadConfig, type LoadConfigResult } from '../lib/config/loader';
import { QueryPanel } from '../panels/query/QueryPanel';

export interface AppProps {
  /** Test seam for /config.json fetch; defaults to globalThis.fetch. */
  readonly fetchFn?: typeof fetch;
}

type AppState =
  | { readonly kind: 'loading' }
  | { readonly kind: 'loaded'; readonly result: LoadConfigResult };

export function App({ fetchFn }: AppProps): JSX.Element {
  const [state, setState] = useState<AppState>({ kind: 'loading' });

  useEffect(() => {
    const fetcher = fetchFn ?? globalThis.fetch.bind(globalThis);
    void loadConfig({ fetchFn: fetcher }).then((result) => {
      setState({ kind: 'loaded', result });
    });
  }, [fetchFn]);

  if (state.kind === 'loading') {
    return (
      <div className="prism-loading" data-testid="loading-state" aria-busy>
        Loading configuration…
      </div>
    );
  }

  const result = state.result;
  if (result.kind === 'error') {
    // ADR-0026 §5: refuse to mount QueryPanel, but still render the
    // chrome with an "(unconfigured)" backend label so the operator
    // sees a coherent page rather than a blank screen. No fetch to
    // /api/v1/query_range is issued because QueryPanel is not mounted.
    return (
      <div className="prism-panel" data-testid="query-panel-disabled">
        <header className="prism-chrome">
          <span className="prism-backend-label" data-testid="backend-label">
            Backend: (unconfigured)
          </span>
        </header>
        <div
          role="alert"
          className="prism-banner prism-banner-warning"
          data-testid="config-error-banner"
        >
          <strong>Configuration is missing.</strong>
          <span> Contact your Prism administrator.</span>
          <pre className="prism-banner-detail">
            {result.error.kind}: {result.error.message}
          </pre>
        </div>
      </div>
    );
  }

  return <Mounted config={result.config} {...(fetchFn !== undefined && { fetchFn })} />;
}

interface MountedProps {
  readonly config: Extract<LoadConfigResult, { kind: 'ok' }>['config'];
  readonly fetchFn?: typeof fetch;
}

function Mounted({ config, fetchFn }: MountedProps): JSX.Element {
  // Set the document title so screen readers and tab labels announce
  // the backend label. WCAG 2.2 SC 2.4.2 — pages have descriptive titles.
  useEffect(() => {
    const original = document.title;
    document.title = `Prism · ${config.backend.label}`;
    return () => {
      document.title = original;
    };
  }, [config.backend.label]);

  return <QueryPanel config={config} {...(fetchFn !== undefined && { fetchFn })} />;
}
