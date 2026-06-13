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

import { defineConfig, devices } from '@playwright/test';

// Playwright config — PARTIALLY IMPLEMENTED. As of prism-echarts-paint-e2e-v0
// the Prism E2E gate runs slices 01 and 03 (the chart paint proof and the
// empty/error states) as real GREEN assertions in headless Chromium;
// `testMatch` (below) allow-lists exactly those two. Slices 02/04/05/06 are
// still scaffold: their `e2e/*.spec.ts` bodies throw UNIMPLEMENTED and stay
// out of `testMatch`, so they do not run yet. The browser projects, the
// Prometheus digest-SSOT, and the per-slice plan below remain the roadmap
// for graduating the rest.
//
// Browser matrix per outcome-kpis.md (scaffold target): Chrome / Firefox
// / Safari latest two stable each, modelled as three Playwright projects.
//
// CRITICAL-3 fix from Forge iter-1 review: the Prometheus image
// digest is the SSOT here; environments.yaml documents the rule
// that this digest MUST equal the digest in
// .github/workflows/ci.yml's gate-11-prism-prometheus-contract
// services block. Bumps are a single atomic commit updating both.
const PROMETHEUS_IMAGE_DIGEST =
  'prom/prometheus@sha256:378f4e03703557d1c6419e6caccf922f96e6d88a530f7431d66a4c4f4b1000fe';
// ^ Prometheus v2.55.0 digest resolved at slice 01e landing via
// `docker pull prom/prometheus:v2.55.0 && docker inspect`. The
// same digest goes into .github/workflows/ci.yml's
// gate-11-prism-prometheus-contract services block; both update
// atomically per environments.yaml `digest_bump_process`.

export default defineConfig({
  testDir: './e2e',
  // testMatch allow-list grows slice by slice as each spec file's
  // UNIMPLEMENTED throws turn into GREEN assertions. Same shape as
  // the Vitest include glob. At slice 06 graduation the testMatch
  // drops and Playwright runs every spec.
  //
  // Per-slice status:
  //   Slice 01 GREEN/graduated: 'slice-01-walking-skeleton.spec.ts'
  //     (prism-echarts-paint-e2e-v0 — the paint proof; perf-KPI blocks
  //      are test.fixme'd, ADR-0075 D5)
  //   Slice 02 scaffold/UNIMPLEMENTED: 'slice-02-time-range-and-relative-presets.spec.ts'
  //   Slice 03 GREEN/graduated: 'slice-03-error-and-empty-states.spec.ts'
  //     (prism-echarts-paint-e2e-v0 — empty/error states; FM2/FM5/FM6
  //      are test.fixme'd, ADR-0075 D5)
  //   Slice 04 scaffold/UNIMPLEMENTED: 'slice-04-auto-refresh.spec.ts'
  //   Slice 05 scaffold/UNIMPLEMENTED: 'slice-05-absolute-time-range-and-permalink.spec.ts'
  //   Slice 06 scaffold/UNIMPLEMENTED: 'slice-06-accessibility.spec.ts'
  testMatch: [
    'slice-01-walking-skeleton.spec.ts',
    'slice-03-error-and-empty-states.spec.ts',
  ],
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI !== undefined ? 1 : 0,
  ...(process.env.CI !== undefined ? { workers: 1 } : {}),
  reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : 'list',

  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },

  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],

  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:5173',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },

  globalSetup: './e2e/global-setup.ts',
  globalTeardown: './e2e/global-teardown.ts',
});

// Re-export the digest constant so e2e/global-setup.ts consumes the
// same SSOT.
export { PROMETHEUS_IMAGE_DIGEST };
