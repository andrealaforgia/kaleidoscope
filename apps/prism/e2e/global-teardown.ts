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

const execAsync = promisify(exec);

// Stop the local Prometheus fixture container started by
// e2e/global-setup.ts. The container name lives in
// process.env.PRISM_PROM_CONTAINER.
export default async function globalTeardown(): Promise<void> {
  const name = process.env.PRISM_PROM_CONTAINER;
  if (!name) return;
  try {
    await execAsync(`docker stop ${name}`);
  } catch {
    // The container may already be gone (CI worker cleanup); ignore.
  }
}
