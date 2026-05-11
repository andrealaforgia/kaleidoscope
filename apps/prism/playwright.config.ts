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

// Playwright config — Gate 7 (Prism E2E across the browser matrix).
// Browser matrix per outcome-kpis.md: Chrome / Firefox / Safari
// latest two stable each, modelled as three Playwright projects.
//
// CRITICAL-3 fix from Forge iter-1 review: the Prometheus image
// digest is the SSOT here; environments.yaml documents the rule
// that this digest MUST equal the digest in
// .github/workflows/ci.yml's gate-11-prism-prometheus-contract
// services block. Bumps are a single atomic commit updating both.
const PROMETHEUS_IMAGE_DIGEST =
  'prom/prometheus@sha256:0000000000000000000000000000000000000000000000000000000000000000';
// ^ Crafter at slice 01 resolves the latest stable Prometheus 2.x
// digest via `docker pull prom/prometheus:latest && docker inspect`
// and replaces the placeholder above. The same digest goes into
// .github/workflows/ci.yml in the same commit. environments.yaml
// `digest_bump_process` documents the procedure.

export default defineConfig({
  testDir: './e2e',
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
