# ADR-0032 — Prism licence headers in TypeScript source files

- **Status**: Accepted
- **Date**: 2026-05-07
- **Author**: `nw-solution-architect` (Morgan, dispatched by Bea)
- **Feature**: `prism` v0
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0031 (workspace tooling), `LICENSING.md` (project-wide
  licence policy)

## Context

Prism is licensed AGPL-3.0-or-later (DISCUSS D4; per `LICENSING.md`'s
platform-component clause). The Rust crates that carry the same licence
(`crates/aperture/`, `crates/sieve/`, `crates/codex/`) include a
file-level docstring at the top of every source file that records the
licence. The pattern is mature in the Rust workspace and is enforced by
a pre-commit check.

Prism's `.ts`/`.tsx` source files need the same enforcement. Two reasons:

1. **AGPL section 5(c)**: when a covered work is conveyed, the
   notice "stating that this License and any [...] terms" applies to
   the work. A file-level header is the contributor-readable form of
   that notice.
2. **Drift defence**: as `apps/prism/` grows beyond v0, contributors
   add files. A pre-commit / lint check that asserts every source
   file carries the header prevents accidental Apache-2.0-licensed
   contributions from leaking into AGPL-covered code without a
   licence-clearance discussion.

This ADR locks: the header text, the file scope (which files carry it
and which do not), the enforcement mechanism, and the migration path
when the licence text needs an update (e.g. copyright-year roll).

## Decision

### 1. Header text

```ts
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

```

The text lives in a single file at `scripts/licence-header-agpl.txt`
(adjacent to the existing Rust licence header file if any). The
ESLint plugin reads from this path; updates touch one file and
propagate via the lint step.

The header ends with a single blank line; the actual code follows
without an extra newline. Prettier's formatting honours this by
default.

### 2. File scope

| File pattern | Header? | Rationale |
|---|---|---|
| `apps/prism/src/**/*.ts` | yes | source code |
| `apps/prism/src/**/*.tsx` | yes | source code |
| `apps/prism/src/**/*.module.css` | no | CSS files; the licence is recorded in `package.json` |
| `apps/prism/tests/**/*.ts` | yes | tests are first-class source |
| `apps/prism/e2e/**/*.ts` | yes | tests are first-class source |
| `apps/prism/index.html` | no | HTML; the licence is in `package.json` and in the `<meta>` tag |
| `apps/prism/public/**` | no | static assets; not source |
| `apps/prism/vite.config.ts` | yes | configuration is source |
| `apps/prism/eslint.config.ts` | yes | configuration is source |
| `apps/prism/tsconfig.json` | no | JSON cannot carry comments |
| `apps/prism/package.json` | no | JSON; licence is the `"license"` field |

Generated files (e.g. anything under `apps/prism/dist/` or
`apps/prism/playwright-report/`) are excluded from ESLint scoping
and therefore from this rule.

### 3. Enforcement: ESLint `eslint-plugin-license-header`

The plugin (referenced in ADR-0031 § 7) reads
`scripts/licence-header-agpl.txt` and asserts every file in scope
opens with that exact text. A failed assertion is an ESLint error; the
pre-commit and CI gates fail.

The plugin auto-fixes when run with `--fix`: missing headers are
prepended. This is the intentionally-permissive ergonomics: a
contributor who creates a new file without the header runs `pnpm lint
--fix` and the header is added. Pre-commit fails closed if the file
still lacks the header at commit time.

### 4. Update flow

When the licence text needs an update (copyright-year roll, version
bump from AGPLv3 to a hypothetical AGPLv4):

1. Update `scripts/licence-header-agpl.txt` with the new text.
2. Run `pnpm -r lint --fix`. The plugin replaces every occurrence in
   scope.
3. Commit the diff under a single PR titled e.g.
   "chore: update AGPL header to 2027 copyright".

The single-source-of-truth shape means licence text drift is
structurally impossible: every file is auto-rewritten on the next
lint-fix run.

### 5. Interaction with the Rust workspace

The Rust crates (Aperture, Sieve, Codex) that share the AGPL licence
follow their own per-crate convention (a Rust file-level docstring
or a `LICENSE` doc-include at the crate root). The Rust enforcement
is independent of Prism's; both carry the same legal weight. The
two enforcement paths converge at `LICENSING.md`'s top-level table.

## Alternatives considered

### Option A (rejected): SPDX short identifier only

```ts
// SPDX-License-Identifier: AGPL-3.0-or-later
```

A single line at the top of every file, matching the SPDX convention.
Argument for: minimal noise, machine-parseable. Argument against
(and the reason this ADR rejects it): the AGPL's section 5(c) calls
for "appropriate copyright notice"; an SPDX-only header omits the
copyright holder, the copyright year, and the warranty disclaimer.
The full header carries the legal weight; the SPDX short form is a
machine-readable index. The plugin can assert SPDX as a separate
rule on top, but the full header is the load-bearing item.

### Option B (rejected): Header lives in the file via a Babel/SWC plugin at build time

A build-time transform that prepends the header at bundle time, not
at source time. Argument for: source files stay header-free, less
contributor friction. Argument against: the source files are also
the licence-distribution unit (a contributor cloning the repo gets
the source). Source-time enforcement is the legal posture; build-time
is a rendering concern.

### Option C (rejected): Pre-commit hook checks; no ESLint rule

A standalone Bash check in `scripts/hooks/pre-commit` that asserts
header presence. Argument for: one fewer ESLint plugin. Argument
against: ESLint runs in CI, in IDEs, on save, on `pnpm lint`. The
pre-commit hook only runs on commit. ESLint catches header drift
earlier in the developer cycle and integrates with the auto-fix
flow.

### Option D (rejected): No header; rely on `package.json :: license` and `LICENSE` file

Argument for: the `license` field is the canonical source. Argument
against: the AGPL's section 5(c) requires the per-file notice. The
top-level `LICENSE` file is necessary but not sufficient; the
file-level header is the legal complement.

## Consequences

### Positive

- **AGPL section 5(c) compliance is structurally enforced**. A new
  source file without the header fails ESLint, fails pre-commit,
  fails CI.
- **Single-source-of-truth licence text**. One file
  (`scripts/licence-header-agpl.txt`); updates propagate via lint-fix.
- **Auto-fix removes contributor friction**. Forgetting the header
  is a `pnpm lint --fix` away from being added.
- **Mirrors the Rust workspace's per-file licence discipline**. Cross-
  language consistency for contributors who move between Rust and TS
  files.

### Negative

- **Every source file carries 13 lines of boilerplate**. Editor
  folding mitigates the visual noise; greppability is unaffected
  (the licence text is not fielded into search results that
  developers actually type).
- **The plugin (`eslint-plugin-license-header`) is a single
  unmaintained-flag risk**. If it goes unmaintained, fall back to a
  hand-rolled ESLint rule (50 lines of TypeScript — the rule is
  simple). The ESLint plugin ecosystem is mature; this is a small
  risk.

### Trade-off summary

13 lines per file pays for AGPL compliance plus drift defence. The
discipline is mature in the Rust workspace; ADR-0032 brings it to
the TS workspace with the same posture.

## Verification

- ESLint runs in pre-commit and CI; every file in scope must pass
  the `license-header/header` rule.
- A Vitest test asserts that a representative sample of source files
  (under `apps/prism/src/`, `apps/prism/tests/`) open with the
  expected header text. The test is a tripwire against accidental
  ESLint disablement.
- Slice 01's first source file commit passes the header check (the
  walking skeleton's `main.tsx` carries the header).
