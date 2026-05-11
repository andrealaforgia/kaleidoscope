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
      // Slice 02 GREEN at micro-slice 02 — picker UI + codec.
      'tests/slice-02-*.test.{ts,tsx}',
      // Slice 03 GREEN at slice 03 — error + empty + malformed-URL.
      'tests/slice-03-*.test.{ts,tsx}',
      // Slice 01 walking skeleton — partial GREEN. Re-add when the
      // QueryPanel-rendering tests get real bodies (slice 02+
      // integration work).
      // Re-add when slice 04 GREEN: 'tests/slice-04-*.test.ts'
      // Re-add when slice 05 GREEN: 'tests/slice-05-*.test.ts'
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
