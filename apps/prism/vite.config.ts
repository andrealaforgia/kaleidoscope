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

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Vite default config (per ADR-0031). Tree-shaking and code-splitting
// rely on Vite's defaults; the bundle-size gate (Gate 8) verifies the
// resulting gzipped JS stays under 300 KB. ECharts modular imports
// (per ADR-0030 §1) avoid pulling the full chart library into the
// bundle.
//
// Dev-mode proxy: the Vite dev server proxies `/api/v1/*` to a local
// Prometheus container on :9090 so the SPA's same-origin posture is
// preserved during development. Production deployment is via the
// operator's reverse proxy per ADR-0027 §5.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api/v1': {
        target: 'http://localhost:9090',
        changeOrigin: true,
      },
    },
  },
  build: {
    target: 'es2022',
    sourcemap: true,
    rollupOptions: {
      output: {
        // Manual chunking strategy: ECharts is large enough that a
        // dynamic-import escape hatch is preserved per ADR-0030 §7.
        // Slice 01 ships ECharts in the main chunk; Slice 06 may flip
        // to lazy if the bundle approaches the 300 KB gate.
      },
    },
  },
});
