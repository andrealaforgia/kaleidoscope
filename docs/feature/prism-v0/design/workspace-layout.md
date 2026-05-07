# Prism v0 — Workspace layout

- **Wave**: DESIGN
- **Author**: `@nw-solution-architect` (Morgan, dispatched by Bea)
- **Date**: 2026-05-07
- **Companion ADRs**: ADR-0031 (workspace tooling), ADR-0032 (licence
  headers), ADR-0026 (component layout)

This document is the operational reference for the directory layout
introduced by Prism v0. ADR-0031 carries the rationale; this document
carries the inventory.

---

## 1. Repository layout (after Prism v0 lands)

```
kaleidoscope/
├── Cargo.toml                     -- Rust workspace manifest (unchanged)
├── pnpm-workspace.yaml            -- TS workspace manifest (NEW)
├── package.json                   -- top-level dev scripts (NEW)
├── pnpm-lock.yaml                 -- TS lockfile, committed (NEW)
├── .npmrc                         -- pnpm posture (NEW)
├── .nvmrc                         -- Node version pin (NEW)
├── rust-toolchain.toml            -- Rust toolchain pin (unchanged)
├── deny.toml                      -- cargo-deny config (unchanged)
├── LICENSING.md                   -- per-component licence table (extended)
├── README.md                      -- repo-level (extended)
├── crates/                        -- Rust crates (unchanged)
│   ├── otlp-conformance-harness/
│   ├── aperture/
│   ├── spark/
│   ├── sieve/
│   └── codex/
├── xtask/                         -- Rust xtasks (unchanged)
├── apps/                          -- TS deployable apps (NEW)
│   └── prism/
├── docs/                          -- (existing) feature, product, presentation
├── scripts/
│   ├── hooks/
│   │   ├── pre-commit             -- extended with TS gates (ADR-0031 § 8)
│   │   └── pre-push
│   └── licence-header-agpl.txt    -- single source of truth for the AGPL header (NEW)
└── .github/
    └── workflows/
        ├── ci.yml                 -- extended with TS job (DEVOPS owns)
        └── ...
```

---

## 2. `apps/prism/` layout

```
apps/prism/
├── package.json                   -- deps, scripts, AGPL declaration
├── tsconfig.json                  -- strict mode, project references
├── vite.config.ts                 -- React plugin, dev proxy, bundle-size guard
├── eslint.config.ts               -- type-checked + boundaries + license-header
├── .prettierrc.json               -- 100 char width, single quotes, trailing commas
├── playwright.config.ts           -- 3 engines: Chromium, Firefox, WebKit
├── vitest.config.ts               -- JSdom environment, RTL setup
├── index.html                     -- single SPA entry
├── public/
│   └── config.json.example        -- shape-only exemplar; operator overrides
├── src/
│   ├── main.tsx                   -- composition root
│   ├── app/
│   │   ├── App.tsx
│   │   └── RootProviders.tsx
│   ├── panels/
│   │   └── query/
│   │       ├── QueryPanel.tsx
│   │       ├── QueryInput.tsx
│   │       ├── RangePicker.tsx
│   │       ├── RefreshPicker.tsx
│   │       ├── RunButton.tsx
│   │       ├── ChartArea.tsx
│   │       ├── StatusLine.tsx
│   │       ├── ErrorBanner.tsx
│   │       ├── EmptyState.tsx
│   │       ├── Footer.tsx
│   │       └── MalformedUrlBanner.tsx
│   ├── lib/
│   │   ├── promql/
│   │   │   ├── client.ts
│   │   │   ├── parse.ts
│   │   │   └── types.ts
│   │   ├── url-state/
│   │   │   ├── codec.ts
│   │   │   ├── result.ts
│   │   │   └── types.ts
│   │   ├── auto-refresh/
│   │   │   ├── reducer.ts
│   │   │   ├── events.ts
│   │   │   ├── effects.ts
│   │   │   └── scheduler.ts
│   │   ├── config/
│   │   │   ├── loader.ts
│   │   │   └── types.ts
│   │   └── echarts/
│   │       ├── buildOption.ts
│   │       ├── EChart.tsx
│   │       ├── instance.ts
│   │       └── palette.ts
│   ├── components/
│   │   ├── Button.tsx
│   │   ├── Dropdown.tsx
│   │   ├── Banner.tsx
│   │   └── FocusRing.tsx
│   └── styles/
│       ├── theme.module.css
│       └── global.css
├── tests/
│   ├── setup.ts                   -- RTL global setup
│   ├── slice-01-walking-skeleton.test.ts
│   ├── slice-02-relative-presets.test.ts
│   ├── slice-03-errors.test.ts
│   ├── slice-04-auto-refresh.test.ts
│   ├── slice-05-absolute-range.test.ts
│   └── slice-06-accessibility.test.ts
└── e2e/
    ├── slice-01-walking-skeleton.spec.ts
    ├── slice-02-relative-presets.spec.ts
    ├── slice-03-errors.spec.ts
    ├── slice-04-auto-refresh.spec.ts
    ├── slice-05-absolute-range.spec.ts
    └── slice-06-accessibility.spec.ts
```

The `setup.ts` under `tests/` is the only RTL bootstrap concern; per-slice
tests import from it. Playwright tests have no shared bootstrap (the
fixture is a real Prometheus container declared in
`playwright.config.ts`'s `globalSetup`).

---

## 3. Tooling discipline

### 3.1 Package manager

`pnpm` (>= 9). Installed via `corepack enable && corepack install`
(no system-wide install required). The repository has `packageManager`
in the top-level `package.json` so corepack picks the version
automatically.

### 3.2 Node version

Node 22 LTS (pinned in `.nvmrc`). Bumps follow the same MSRV-creep
posture the Rust workspace uses.

### 3.3 Lockfile

`pnpm-lock.yaml` is committed. CI installs via
`pnpm install --frozen-lockfile`; a drift between `package.json` and
the lockfile fails CI.

### 3.4 Pinning policy

Every dependency in `apps/prism/package.json` is exact-pinned (no
`^`, no `~`). The `.npmrc` setting `save-exact=true` enforces this
on `pnpm add`. This mirrors Codex / Spark's `=0.27` exact-minor
pinning posture (ADR-0024 § 3).

### 3.5 Linter

ESLint with `@typescript-eslint/recommended-type-checked` profile,
`eslint-plugin-boundaries` for module-boundary enforcement (ADR-0026),
and `eslint-plugin-license-header` for the AGPL header (ADR-0032).
Configured in `eslint.config.ts` (flat config, ESLint 9+).

### 3.6 Formatter

Prettier with config: `printWidth: 100`, `singleQuote: true`,
`trailingComma: 'all'`, `semi: true`. The same format settings are
honoured by the IDE (every contributor's editor reads `.prettierrc.json`).

### 3.7 Type system

TypeScript 5.7+ in `strict: true` mode. No `any` allowed (enforced by
`@typescript-eslint/no-explicit-any`). No `as` casts without a comment
explaining why (enforced by a custom ESLint rule or by `// eslint-
disable-next-line` discipline).

### 3.8 Test runners

- **Vitest**: unit tests under `apps/prism/tests/` and
  component tests adjacent to source. JSdom environment. RTL
  on top.
- **Playwright**: E2E under `apps/prism/e2e/`. Three engines:
  Chromium, Firefox, WebKit. Real Prometheus container in
  `globalSetup`.

### 3.9 Bundler

Vite 5 (latest stable at v0). Dev mode runs the SPA on
`http://localhost:5173/`; the dev proxy forwards `/api/v1/*` to
`http://localhost:9090` (the demo Prometheus). Production build
emits a single bundle plus the `index.html` plus the static assets.

### 3.10 Bundle-size gate

The CI job runs `pnpm -r build` and asserts the gzipped size of the
main JS bundle is under 300 KB. Implementation: a script in
`apps/prism/scripts/check-bundle-size.ts` (DEVOPS owns the script;
DESIGN locks the gate value).

---

## 4. Pre-commit hook

The existing `scripts/hooks/pre-commit` (Rust gates only) gains a TS
section. The TS section runs `pnpm -r lint`, `pnpm -r typecheck`,
`pnpm -r test`. It is gated on `apps/prism/package.json` existing so
contributors who only touch Rust do not pay the TS gate cost.

The hook is best-effort feedback; CI is the ground truth. The
Kaleidoscope no-CI-gate posture (per project memory) means CI
failures do not block merge but do trigger fix-forward.

---

## 5. CI workflow extension (DEVOPS owns)

`.github/workflows/ci.yml` gains a parallel `prism` job that runs:

1. `pnpm install --frozen-lockfile`
2. `pnpm -r lint`
3. `pnpm -r typecheck`
4. `pnpm -r test`
5. `pnpm -r build` (with bundle-size assertion)
6. `pnpm -r e2e` (Playwright against local Prometheus container)

Existing Rust jobs unchanged. The two job groups run in parallel.

---

## 6. Licence and licensing

| Path | Licence | Mechanism |
|---|---|---|
| `apps/prism/src/**/*.{ts,tsx}` | AGPL-3.0-or-later | File-level header (ADR-0032) + `package.json :: license` |
| `apps/prism/tests/**/*.ts` | AGPL-3.0-or-later | File-level header + parent package |
| `apps/prism/e2e/**/*.ts` | AGPL-3.0-or-later | File-level header + parent package |
| `apps/prism/package.json` | AGPL-3.0-or-later | `license` field |
| `apps/prism/index.html` | AGPL-3.0-or-later | `<meta name="license">` tag, plus inheritance from `package.json` |
| `apps/prism/public/config.json.example` | (data, not source) | n/a |
| `apps/prism/playwright-report/**` | (generated) | excluded |
| `apps/prism/dist/**` | (generated) | excluded |
| `pnpm-workspace.yaml`, top-level `package.json` | AGPL-3.0-or-later | top-level `license` field |
| `Cargo.toml`, `crates/**` | (unchanged) | per-crate, see `LICENSING.md` |

`LICENSING.md` gains a row for `apps/prism/` and notes the file-level
header convention. The two enforcement paths (Rust file-level
docstring + TS ESLint rule) converge at this single document.

---

## 7. Migration steps for the crafter

When DELIVER opens Slice 01, the crafter:

1. Creates `pnpm-workspace.yaml` with `apps/*`.
2. Creates top-level `package.json` (per ADR-0031 § 3).
3. Creates `.npmrc`, `.nvmrc`, `apps/prism/package.json` per ADR-0031
   §§ 4-6.
4. Runs `pnpm install` to generate `pnpm-lock.yaml`.
5. Creates `apps/prism/tsconfig.json`, `vite.config.ts`,
   `eslint.config.ts`, `.prettierrc.json`, `playwright.config.ts`,
   `vitest.config.ts`.
6. Creates `scripts/licence-header-agpl.txt` per ADR-0032 § 1.
7. Extends `scripts/hooks/pre-commit` per ADR-0031 § 8.
8. Updates `LICENSING.md` with the `apps/prism/` row.
9. Implements Slice 01's walking skeleton against the layout in
   ADR-0026 § 1.

The DEVOPS-owned items (CI workflow, bundle-size script, Playwright
fixture) land in parallel via the `@nw-platform-architect` handoff.
