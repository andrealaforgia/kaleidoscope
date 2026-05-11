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

// ESLint flat config — ADR-0031 §7 (boundaries plugin enforces
// module dependency direction). The AGPL licence-header enforcement
// moves entirely to the runtime invariant test at
// `tests/invariant-licence-headers.test.ts` because the
// `eslint-plugin-license-header@0.6.1` package is incompatible with
// the line-comment header style: it expects a single block comment
// and aggressively duplicates the header on --fix when the existing
// header is in line-comment form. The test catches drift; the
// double-enforcement was belt-and-braces, not load-bearing. ADR-0032
// gains a post-DELIVER amendment naming the new structural posture.
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
    },
    settings: {
      'boundaries/elements': [
        { type: 'panels', pattern: 'src/panels/*' },
        { type: 'lib-promql', pattern: 'src/lib/promql' },
        { type: 'lib-config', pattern: 'src/lib/config' },
        { type: 'lib-echarts', pattern: 'src/lib/echarts' },
        { type: 'lib-url-state', pattern: 'src/lib/url-state' },
        { type: 'lib-auto-refresh', pattern: 'src/lib/auto-refresh' },
        { type: 'components', pattern: 'src/components' },
        { type: 'app', pattern: 'src/app' },
      ],
    },
    rules: {
      // ADR-0031 §7 — module-graph rules.
      'boundaries/element-types': [
        'error',
        {
          default: 'disallow',
          rules: [
            { from: 'panels', allow: ['lib-promql', 'lib-config', 'lib-echarts', 'lib-url-state', 'lib-auto-refresh', 'components'] },
            { from: 'lib-promql', allow: ['lib-url-state'] },
            { from: 'lib-config', allow: [] },
            { from: 'lib-echarts', allow: ['lib-promql', 'lib-url-state'] },
            { from: 'lib-url-state', allow: [] },
            { from: 'lib-auto-refresh', allow: ['lib-promql', 'lib-url-state'] },
            { from: 'app', allow: ['panels', 'lib-promql', 'lib-config', 'lib-url-state', 'lib-auto-refresh', 'lib-echarts', 'components'] },
            { from: 'components', allow: [] },
          ],
        },
      ],
    },
  },
];
