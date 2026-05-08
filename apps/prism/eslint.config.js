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

import tseslint from '@typescript-eslint/eslint-plugin';
import tseslintParser from '@typescript-eslint/parser';
import boundaries from 'eslint-plugin-boundaries';
import licenseHeader from 'eslint-plugin-license-header';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const headerPath = fileURLToPath(
  new URL('../../scripts/licence-header-agpl.txt', import.meta.url),
);
const headerLines = readFileSync(headerPath, 'utf-8').split('\n').filter(Boolean);

// ESLint flat config — ADR-0031 §7 (boundaries plugin enforces
// module dependency direction) + ADR-0032 (licence-header plugin
// auto-fixes the AGPL header on every TS source file).
export default [
  {
    files: ['src/**/*.{ts,tsx}'],
    languageOptions: {
      parser: tseslintParser,
      parserOptions: {
        project: './tsconfig.json',
        ecmaFeatures: { jsx: true },
      },
    },
    plugins: {
      '@typescript-eslint': tseslint,
      boundaries,
      'license-header': licenseHeader,
    },
    settings: {
      'boundaries/elements': [
        // Driving (panels): may import from lib + components
        { type: 'panels', pattern: 'src/panels/*' },
        // Driven adapters: may import from lib only (no React infrastructure)
        { type: 'lib-promql', pattern: 'src/lib/promql' },
        { type: 'lib-config', pattern: 'src/lib/config' },
        { type: 'lib-echarts', pattern: 'src/lib/echarts' },
        // Pure cores: must not import side-effecting modules
        { type: 'lib-url-state', pattern: 'src/lib/url-state' },
        { type: 'lib-auto-refresh', pattern: 'src/lib/auto-refresh' },
        // Atoms (re-usable UI primitives)
        { type: 'components', pattern: 'src/components' },
        // App composition root
        { type: 'app', pattern: 'src/app' },
      ],
    },
    rules: {
      // ADR-0032 — AGPL header on every src file, auto-fixable
      'license-header/header': ['error', headerLines],
      // ADR-0031 §7 — module-graph rules
      'boundaries/element-types': [
        'error',
        {
          default: 'disallow',
          rules: [
            // Panels can compose lib + components
            { from: 'panels', allow: ['lib-promql', 'lib-config', 'lib-echarts', 'lib-url-state', 'lib-auto-refresh', 'components'] },
            // Adapters can use the pure cores
            { from: 'lib-promql', allow: ['lib-url-state'] },
            { from: 'lib-config', allow: [] },
            { from: 'lib-echarts', allow: ['lib-promql', 'lib-url-state'] },
            // Pure cores import nothing else
            { from: 'lib-url-state', allow: [] },
            { from: 'lib-auto-refresh', allow: ['lib-promql', 'lib-url-state'] },
            // App composition root wires everything
            { from: 'app', allow: ['panels', 'lib-promql', 'lib-config', 'lib-url-state', 'lib-auto-refresh', 'lib-echarts', 'components'] },
            // Components are leaves
            { from: 'components', allow: [] },
          ],
        },
      ],
    },
  },
];
