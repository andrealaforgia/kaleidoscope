# ADR-0031 — Prism workspace layout and tooling

- **Status**: Accepted
- **Date**: 2026-05-07
- **Author**: `nw-solution-architect` (Morgan, dispatched by Bea)
- **Feature**: `prism` v0
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0026 (Prism component layout), ADR-0032 (licence
  headers), ADR-0005 (CI contract — workspace gates), ADR-0024 (Codex
  dependency pinning — exact-minor pattern referenced here)

## Context

Prism is the project's first frontend feature. The Kaleidoscope repo
has been Rust-only until now: a Cargo workspace at the root with
`crates/<name>/` members and an `xtask/` member. Adding a TypeScript
SPA introduces a second build system (npm semantics, pnpm tooling,
Vite, Vitest, Playwright, ESLint, Prettier) inside the same git repo.

The two build systems must coexist without:

- The Rust `cargo` invocations seeing TS files as members.
- The TS `pnpm` invocations seeing Rust files as packages.
- The pre-commit hook (Rust gates only at present) silently skipping
  the TS gates.
- The CI workflow (`.github/workflows/`) running Rust gates on TS-only
  PRs and vice versa.

The pre-locked decisions cover most of the technology choices: pnpm
package manager, `apps/prism/` location, exact-minor pinning for
critical deps (mirroring Codex/Spark's `=0.27` style for opentelemetry-
proto). This ADR locks the workspace structure, the lockfile and
node-version posture, the pre-commit hook addition, the CI contract
extension, and the language-appropriate enforcement tooling.

## Decision

### 1. Top-level structure

```
kaleidoscope/                   <-- repo root
├── Cargo.toml                  -- existing Rust workspace (unchanged)
├── pnpm-workspace.yaml         -- NEW: TS workspace declaration
├── package.json                -- NEW: top-level scripts + dev tooling
├── pnpm-lock.yaml              -- NEW: committed
├── .npmrc                      -- NEW: pnpm-strict settings
├── .nvmrc                      -- NEW: node version pin
├── crates/                     -- existing Rust crates (unchanged)
├── apps/
│   └── prism/                  -- NEW: Prism SPA
└── xtask/                      -- existing Rust xtasks (unchanged)
```

Two workspace files: `Cargo.toml` for Rust, `pnpm-workspace.yaml` for
TS. They do not conflict; each tool reads only its own file.

### 2. `pnpm-workspace.yaml` content

```yaml
# pnpm-workspace.yaml

packages:
  - 'apps/*'
  # 'packages/*' deferred — no shared TS libraries at v0. Add when the
  # first cross-app shared package emerges (likely Loom v0 or Aegis
  # frontend).
```

### 3. Top-level `package.json` content

```jsonc
{
  "name": "kaleidoscope",
  "private": true,
  "version": "0.0.0",
  "license": "AGPL-3.0-or-later",
  "engines": {
    "node": ">=22.0.0",
    "pnpm": ">=9.0.0"
  },
  "scripts": {
    "lint": "pnpm -r lint",
    "format": "pnpm -r format",
    "format:check": "pnpm -r format:check",
    "typecheck": "pnpm -r typecheck",
    "test": "pnpm -r test",
    "build": "pnpm -r build",
    "e2e": "pnpm -r e2e"
  },
  "devDependencies": {}
}
```

The top-level `package.json` is `private: true`; it never publishes.
`pnpm -r <script>` runs the script in every workspace member, in
topological order. Each `apps/<name>/package.json` declares its own
script implementations.

### 4. `apps/prism/package.json` content (sketch)

```jsonc
{
  "name": "@kaleidoscope/prism",
  "version": "0.1.0",
  "private": true,
  "license": "AGPL-3.0-or-later",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview",
    "lint": "eslint src tests e2e --max-warnings=0",
    "format": "prettier --write src tests e2e",
    "format:check": "prettier --check src tests e2e",
    "typecheck": "tsc --noEmit",
    "test": "vitest run",
    "test:watch": "vitest",
    "e2e": "playwright test"
  },
  "dependencies": {
    "react": "=19.0.0",
    "react-dom": "=19.0.0",
    "react-router-dom": "=7.0.0",
    "echarts": "=5.5.1"
  },
  "devDependencies": {
    "@playwright/test": "=1.48.0",
    "@testing-library/react": "=16.0.1",
    "@testing-library/user-event": "=14.5.2",
    "@types/react": "=19.0.0",
    "@types/react-dom": "=19.0.0",
    "@typescript-eslint/eslint-plugin": "=8.16.0",
    "@typescript-eslint/parser": "=8.16.0",
    "@vitejs/plugin-react": "=4.3.4",
    "eslint": "=9.16.0",
    "eslint-plugin-boundaries": "=5.0.1",
    "eslint-plugin-license-header": "=0.6.1",
    "jsdom": "=25.0.1",
    "prettier": "=3.4.0",
    "typescript": "=5.7.2",
    "vite": "=5.4.11",
    "vitest": "=2.1.8"
  }
}
```

Every dependency uses an **exact** version pin (`=X.Y.Z` style; in
npm grammar this is just `X.Y.Z`, but the intent is the same as the
Cargo `=0.27` style ADR-0024 locks: no semver-ranges-without-permission).
Updates are explicit single-PR events, not silent transitive drift.

The exact versions in this ADR are illustrative; the crafter resolves
the latest-stable values at `pnpm add` time and pins them. The
**discipline** is exact pinning; the **values** are an implementation
detail of the install.

### 5. `.npmrc` content

```ini
# .npmrc
auto-install-peers=true
strict-peer-dependencies=true
save-exact=true
package-manager-strict=true
```

`save-exact=true` is the structural enforcement of the exact-pin
discipline above: even if a developer types `pnpm add foo`, the
resulting `package.json` entry is `"foo": "1.2.3"`, not `"foo": "^1.2.3"`.

`strict-peer-dependencies=true` matches Cargo's "all transitives must
agree" posture.

### 6. `.nvmrc` content

```
22
```

Node 22 LTS is the v0 floor (matches the Vite 5 / Vitest 2 minimum at
the time of writing). Bumps follow the same MSRV-creep posture the
Rust workspace uses (ADR-0005 amendments): the floor moves with
ecosystem reality, not with feature uptake.

### 7. ESLint configuration

```ts
// apps/prism/eslint.config.ts (sketch)

import tseslint from '@typescript-eslint/eslint-plugin';
import boundaries from 'eslint-plugin-boundaries';
import licenseHeader from 'eslint-plugin-license-header';

export default [
  {
    files: ['src/**/*.{ts,tsx}', 'tests/**/*.ts', 'e2e/**/*.ts'],
    plugins: {
      '@typescript-eslint': tseslint,
      boundaries,
      'license-header': licenseHeader,
    },
    rules: {
      // type-checked profile
      ...tseslint.configs['recommended-type-checked'].rules,
      // ADR-0026 module boundaries
      'boundaries/element-types': ['error', {
        default: 'disallow',
        rules: [
          { from: 'app',        allow: ['panels', 'components', 'lib'] },
          { from: 'panels',     allow: ['components', 'lib'] },
          { from: 'lib',        allow: ['lib'] },
          { from: 'components', allow: ['components'] },
          { from: 'tests',      allow: ['*'] },
          { from: 'e2e',        allow: ['*'] },
        ],
      }],
      // ADR-0032 licence header
      'license-header/header': ['error', './scripts/licence-header.txt'],
    },
  },
];
```

The `boundaries` plugin is the language-appropriate enforcement of
ADR-0026's module split. It is the TypeScript analogue of ArchUnit /
import-linter / pytest-archon — principle 11 requires it.

### 8. Pre-commit hook addition

The existing `scripts/hooks/pre-commit` is a Bash script that runs Rust
gates only. Extend it:

```bash
# scripts/hooks/pre-commit (added section)

# Step 5: pnpm lint + typecheck + test (TS gates)
if [ -f apps/prism/package.json ]; then
  echo "→ pnpm -r lint && pnpm -r typecheck && pnpm -r test  (TS gates)"
  if command -v pnpm >/dev/null 2>&1; then
    if ! pnpm -r lint; then
      red "[fail] pnpm lint"
      exit 1
    fi
    if ! pnpm -r typecheck; then
      red "[fail] pnpm typecheck"
      exit 1
    fi
    if ! pnpm -r test; then
      red "[fail] pnpm test"
      exit 1
    fi
  else
    yellow "[skip] pnpm not installed; install: corepack enable && corepack install"
  fi
fi
```

The hook is conditional on `apps/prism/package.json` existing so
contributors who only touch Rust files do not pay the TS gate cost.
A more refined "modified-files-only" pre-commit is a v1+ optimisation;
v0 takes the simpler "always run" shape and accepts the TS gate cost
on Rust-only commits (typically ~3-5 seconds).

### 9. CI contract extension

CI's existing workflow (Rust-only) gains a parallel TS job:

```yaml
# .github/workflows/ci.yml (sketch — DEVOPS owns this, design records it)

jobs:
  rust:
    # existing job, unchanged.
  prism:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version-file: .nvmrc
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - run: pnpm install --frozen-lockfile
      - run: pnpm -r lint
      - run: pnpm -r typecheck
      - run: pnpm -r test
      - run: pnpm -r build
      - run: pnpm -r e2e
      # bundle-size assertion lives in pnpm -r build (a custom Vite
      # plugin or a separate script that diffs the gzipped bundle
      # against the 300 KB cap).
```

CI runs the two job groups (Rust + Prism) in parallel; both must be
green for merge. The pre-commit gates are best-effort feedback per
the Kaleidoscope no-CI-gate posture; CI is feedback, not a gate (per
project memory). Failures still trigger the fix-forward / post-merge
correction ritual.

### 10. Cargo and pnpm coexistence

The two workspaces do not share a dependency graph. They share:

- **The git repository** (one history, one PR queue).
- **The pre-commit hook** (Rust gates + TS gates).
- **The CI workflow** (parallel jobs).
- **The licence**: per `LICENSING.md`, both Rust crates and Prism
  follow the AGPL-vs-Apache split based on platform-vs-SDK role.
  Prism is platform / operator-facing → AGPL.

They do NOT share:

- **The lockfile** (`Cargo.lock` and `pnpm-lock.yaml` are independent).
- **The dependency policy** (`cargo deny`'s `deny.toml` does not
  govern npm dependencies; pnpm's `audit` and licence checks are
  separate).
- **The version cadence** (Prism v0 ships independently of any Rust
  crate; Cargo workspace dependencies do not block Prism builds).

## Alternatives considered

### Option A (rejected): Separate git repo for Prism

A `kaleidoscope-prism` repo. Argument for: zero coexistence work,
each repo has one build system. Argument against (and the reason
this ADR rejects it): Prism's contract surface (URL params, config
shape, query invariants) is part of the Kaleidoscope-wide product
definition. Operators expect a single release line. The single repo
matches the project's Conway-law shape (one operator, one designer,
one developer — Andrea — across all features).

### Option B (rejected): npm workspaces instead of pnpm

npm has built-in workspaces since 7.x. Argument for: no extra
package-manager install. Argument against: pnpm's content-addressed
store, strict peer dependency resolution, and the `pnpm -r` recursive
script semantics are operationally tighter. The pre-locked decision
is pnpm; this ADR records the rationale.

### Option C (rejected): Bun

Bun has the most momentum among new JS runtimes. Argument for:
faster install, faster test, integrated bundler. Argument against:
Vite plugin compatibility is still evolving (per the pre-locked
rationale); Bun's TypeScript type-resolution surface lags pnpm's
in subtle ways. v0 picks the safer pnpm; revisit at v1.

### Option D (rejected): Nx or Turborepo monorepo orchestrator

Nx and Turborepo add task-graph orchestration, remote caching, and
configurable pipelines. Argument for: faster CI for large monorepos.
Argument against: Prism v0 is one TS package; the orchestrator's
benefits land at scale. Adding either at v0 is operational complexity
without payoff. Add when the workspace has 3+ TS packages; defer to
v0.x or v1.

## Consequences

### Positive

- **Two coexistent workspaces, one git repo**. Operators see one
  Kaleidoscope release line; contributors see one PR queue.
- **Exact-version pinning across both ecosystems**. No transitive
  drift from semver-range surprises; bumps are explicit single-PR
  events.
- **Module boundaries enforced**. `eslint-plugin-boundaries` is the
  TS analogue of ArchUnit; ADR-0026's split is an automated CI gate,
  not a discipline.
- **Pre-commit hook is contributor-friendly**. Rust-only contributors
  pay a few seconds; full-stack contributors run everything locally
  before push.

### Negative

- **Two lockfiles, two dependency policies**. `cargo deny` audits
  Rust; `pnpm audit` audits npm. Neither covers the other. The
  licence-audit posture (LICENSING.md) is the single source of truth
  for compliance.
- **CI workflow doubles in size**. The Rust job and the Prism job both
  run on every PR. Mitigation: the jobs run in parallel; total wall
  time is bounded by the slower job (typically Rust, given mutation
  testing).
- **Node-version drift**. Node bumps follow ecosystem reality; CI must
  stay current. Same MSRV-creep posture the Rust workspace already
  has.

### Trade-off summary

The two-workspace shape is the smallest division that keeps the two
build systems honest. The alternatives (separate repo, npm-only,
Bun, Nx) trade simplicity in one dimension for friction in another;
the chosen shape minimises friction across the dimensions that matter
to a single-operator project (contributor pace, CI feedback, release
cadence, licence audit).

## Verification

- A CI assertion runs `pnpm install --frozen-lockfile`; failure means
  someone forgot to commit the lockfile.
- A CI assertion runs the bundle-size script and fails the job if
  the gzipped JS exceeds 300 KB (per Slice 06 / DEVOPS).
- A pre-commit local run on a pristine clone passes within ~30 seconds
  (Rust `cargo test` + TS `pnpm test`).
- Module-boundary regressions caught at CI time via
  `eslint-plugin-boundaries` errors.
