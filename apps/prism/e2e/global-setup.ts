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

import { exec } from 'node:child_process';
import { promisify } from 'node:util';
import { PROMETHEUS_IMAGE_DIGEST } from '../playwright.config';

const execAsync = promisify(exec);

// Strategy C "real local" Prometheus container fixture per ADR-0027
// and DEVOPS environments.yaml. The PROMETHEUS_IMAGE_DIGEST is the
// SSOT shared with .github/workflows/ci.yml's gate-11 services
// block; the digest_pin_sync_rule in environments.yaml documents
// the bump procedure.
async function waitForReady(url: string, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url);
      if (res.status === 200) return;
    } catch {
      // ignore; retry
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error(`Prometheus did not become ready within ${timeoutMs}ms (url=${url})`);
}

export default async function globalSetup(): Promise<void> {
  const containerName = `prism-prom-fixture-${Date.now()}`;
  const cwd = process.cwd();
  const cmd = `docker run --rm -d \
    --name ${containerName} \
    -p 9090:9090 \
    -v ${cwd}/e2e/fixtures/prometheus.yml:/etc/prometheus/prometheus.yml:ro \
    ${PROMETHEUS_IMAGE_DIGEST}`;
  await execAsync(cmd);
  process.env.PRISM_PROM_CONTAINER = containerName;
  await waitForReady('http://localhost:9090/-/ready', 30_000);
}
