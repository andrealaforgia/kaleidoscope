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

// @vitest-environment node
//
// Invariant — AGPL-3.0-or-later licence header on every TS / TSX
// source file under `apps/prism/src/`.
//
// Per ADR-0032, every TypeScript source file in Prism's runtime tree
// MUST start with the canonical AGPL header read from the SSOT at
// `scripts/licence-header-agpl.txt`. ESLint Gate 9 enforces the
// requirement at PR time via `eslint-plugin-license-header`; this
// runtime Vitest test is the behavioural belt-and-braces, run inside
// Gate 6.
//
// The test exists to catch the case where ESLint is bypassed (e.g.
// the rule is silently disabled, the plugin is removed, the SSOT
// file is renamed without updating the ESLint config). Two layers
// of enforcement; both must pass.

import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = fileURLToPath(new URL('../../..', import.meta.url));
const prismSrc = join(repoRoot, 'apps/prism/src');
const headerSsot = join(repoRoot, 'scripts/licence-header-agpl.txt');

let canonicalHeader: string;

beforeAll(() => {
  // The SSOT header file is read once. The crafter at DELIVER's first
  // slice writes both the SSOT file and ESLint's reference to it.
  canonicalHeader = readFileSync(headerSsot, 'utf-8');
});

function* walk(dir: string): Generator<string> {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const s = statSync(full);
    if (s.isDirectory()) {
      yield* walk(full);
    } else if (s.isFile() && (full.endsWith('.ts') || full.endsWith('.tsx'))) {
      yield full;
    }
  }
}

describe('Invariant — AGPL header on every TS/TSX source file (ADR-0032)', () => {
  it('every .ts and .tsx file under apps/prism/src/ starts with the canonical AGPL header', () => {
    const violations: string[] = [];

    for (const file of walk(prismSrc)) {
      const contents = readFileSync(file, 'utf-8');
      if (!contents.startsWith(canonicalHeader)) {
        violations.push(relative(repoRoot, file));
      }
    }

    expect(violations, `files missing the AGPL header:\n${violations.join('\n')}`).toEqual([]);
  });

  it('the canonical header file at scripts/licence-header-agpl.txt exists and is non-empty', () => {
    expect(canonicalHeader.length).toBeGreaterThan(0);
    // The SSOT names the full "GNU Affero General Public License";
    // acronym "AGPL" appears in the package.json licence field and
    // in ADR-0032 §2 but not necessarily in the file-level header
    // boilerplate. The Affero match below is the binding check.
    expect(canonicalHeader).toMatch(/affero/i);
  });

  it('the canonical header references the GNU AGPL version 3', () => {
    // ADR-0032 locks "AGPL-3.0-or-later"; the SSOT must name version 3.
    expect(canonicalHeader).toMatch(/version 3/i);
  });

  it('the header is at least 12 lines long (matches the canonical AGPL boilerplate)', () => {
    // Defence against accidental truncation: the canonical header is the
    // 15-line GNU-recommended block. A truncated 3-line header would still
    // mention "AGPL" but lose the warranty / link clauses.
    const lines = canonicalHeader.split('\n').filter((l) => l.length > 0);
    expect(lines.length).toBeGreaterThanOrEqual(12);
  });
});

// =============================================================================
// Anti-cargo-culting: the test does NOT enforce headers on test files
// =============================================================================
//
// Test files (apps/prism/tests/, apps/prism/e2e/) get the same header
// as a project-wide consistency choice (Bea writes them with the
// header), but the structural enforcement is on src/ only. ADR-0032
// scopes the header requirement to "every Prism runtime source file";
// test files are covered by the convention (and ESLint's plugin
// applies to them too if the operator wants), but the test above is
// intentionally not over-broad.

describe('Invariant scope — runtime tree only', () => {
  it('the test scans apps/prism/src/ exclusively (not tests/ nor e2e/)', () => {
    // Sanity check: walk(prismSrc) never reaches tests/ or e2e/.
    for (const file of walk(prismSrc)) {
      expect(file).not.toMatch(/\/apps\/prism\/(tests|e2e)\//);
    }
  });
});
