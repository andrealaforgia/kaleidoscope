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

import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

// Vitest config — Gate 6 (Prism unit + integration tests).
// Per the DISTILL test strategy: 70% unit, 20% integration, 10% E2E.
// Vitest covers the 90%; Playwright (separate config) covers the 10%.
//
// Environment: jsdom for component tests; pure-function tests
// (codec, buildOption, reducer) need no DOM and run in Node.
export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: false,
    setupFiles: ['tests/setup.ts'],
    // Allow-list grows slice by slice as DELIVER turns each test
    // file's UNIMPLEMENTED throws into GREEN assertions. Same shape
    // as the Rust `cargo test --exclude <crate>` posture during
    // DISTILL/DELIVER. At slice 06 graduation the allow-list drops
    // and the include glob widens to `tests/**/*.test.{ts,tsx}`.
    include: [
      // Invariants — always-GREEN cross-cutting tests.
      'tests/invariant-*.test.ts',
      // ADR-0075 D1 — paint-signal decision + initial-state coverage
      // (prism-echarts-paint-e2e-v0); the Gate 10 mutation anchor for
      // the non-empty-series predicate.
      'tests/paint-signal.test.tsx',
      // Slice 02 GREEN at micro-slice 02 — picker UI + codec.
      'tests/slice-02-*.test.{ts,tsx}',
      // Slice 03 GREEN at slice 03 — error + empty + malformed-URL.
      'tests/slice-03-*.test.{ts,tsx}',
      // Slice 04 GREEN at slice 04 — auto-refresh reducer.
      'tests/slice-04-*.test.{ts,tsx}',
      // Slice 05 GREEN at slice 05 — absolute time-range codec.
      'tests/slice-05-*.test.{ts,tsx}',
      // Traces data-access client — find-failed-traces + trace-with-logs
      // foundation for the upcoming linked-view screen (experimentable-stack-v0).
      'tests/traces-client.test.ts',
      // Logs data-access client — body-contains / min-severity symptom
      // search foundation for the logs-search-and-pivot screen.
      'tests/logs-client.test.ts',
      // Slice 08 — the logs search view + pivot to the trace WHERE+WHY.
      'tests/slice-08-*.test.{ts,tsx}',
      // Slice 07 — the linked view: routing + TraceExplorerPanel
      // (find failed traces, see spans + correlated logs on one screen).
      'tests/slice-07-*.test.{ts,tsx}',
      // Slice 09 — the identifier journey: attribute search (attr_key +
      // attr_value) narrows the crowd to one customer's traces.
      'tests/slice-09-*.test.{ts,tsx}',
      // Slice 01 walking skeleton — partial GREEN. Re-add when the
      // QueryPanel-rendering tests get real bodies (slice 02+
      // integration work).
      // (slice 06 is Playwright-only; no Vitest file.)
    ],
    exclude: ['e2e/**', 'node_modules/**'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov'],
      include: ['src/**/*.ts', 'src/**/*.tsx'],
      exclude: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
    },
  },
});
