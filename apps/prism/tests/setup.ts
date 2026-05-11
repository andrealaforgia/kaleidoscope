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

// Vitest setup — runs once per test file before the test bodies.
// Polyfills the jsdom environment for the bits ECharts touches at
// init time: HTMLCanvasElement.getContext (jsdom returns
// "Not implemented" by default). The chart never actually paints
// in JSdom; ECharts' init path probes the context and bails if it
// is null. ADR-0030 §3 documents this trade-off: integration tests
// mount the real EChart wrapper but the canvas itself stays inert;
// visual assertions about the chart are covered by Playwright in
// real browsers, not Vitest in jsdom.

import { afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';

// Auto-cleanup React Testing Library mounts between tests. Without
// this, render() calls leak into subsequent tests' queries and
// `getByTestId` returns multiple matches.
afterEach(() => {
  cleanup();
});

// Stub canvas getContext to satisfy ECharts' probe. Guarded so
// node-env tests (e.g. invariant-licence-headers) which don't load
// jsdom do not blow up referencing HTMLCanvasElement.
if (typeof HTMLCanvasElement !== 'undefined') {
  HTMLCanvasElement.prototype.getContext = function (): null {
    return null;
  };
}

// Stub matchMedia for prefersReducedMotion checks in buildOption.
if (typeof window !== 'undefined' && typeof window.matchMedia !== 'function') {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: (query: string): MediaQueryList =>
      ({
        matches: false,
        media: query,
        onchange: null,
        addListener: (): void => undefined,
        removeListener: (): void => undefined,
        addEventListener: (): void => undefined,
        removeEventListener: (): void => undefined,
        dispatchEvent: (): boolean => false,
      }) as MediaQueryList,
  });
}

// Stub ResizeObserver: ECharts uses it for resize handling.
if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = class {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  } as unknown as typeof ResizeObserver;
}
