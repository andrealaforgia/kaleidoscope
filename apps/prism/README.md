# Prism

Kaleidoscope's operator-facing observability SPA. v0 ships a single
PromQL query panel against an OTel-compatible Prometheus or Mimir
backend, serving the on-call operator's incident-time
"see-the-shape-of-the-signal" job.

Spec: `docs/feature/prism-v0/`. ADRs: 0026 through 0032.

## Layout

- `src/main.tsx` — entry point.
- `src/app/` — composition root.
- `src/panels/query/` — the QueryPanel and its sub-components.
- `src/lib/promql/` — backend HTTP client (driven adapter).
- `src/lib/url-state/` — URL codec (pure functions).
- `src/lib/auto-refresh/` — auto-refresh state machine
  (pure reducer + scheduler effect type).
- `src/lib/echarts/` — ECharts integration (`buildOption` pure
  function + `<EChart>` imperative wrapper).
- `src/lib/config/` — `/config.json` loader (driven adapter).
- `src/components/` — re-usable UI atoms.
- `tests/` — Vitest unit + integration tests.
- `e2e/` — Playwright end-to-end specs.

## Scripts

- `pnpm dev` — Vite dev server with `/api/v1` proxy to local
  Prometheus on port 9090.
- `pnpm build` — TypeScript build + Vite bundle. Output at `dist/`.
- `pnpm lint` — ESLint flat config (boundaries + license-header).
- `pnpm format:check` — Prettier check.
- `pnpm typecheck` — `tsc --noEmit`.
- `pnpm vitest` — Vitest unit + integration suite.
- `pnpm playwright` — Playwright E2E suite.
- `pnpm bundle-size` — Gate 8 bundle-size check.
- `pnpm stryker` — StrykerJS mutation testing (Gate 10 calls
  `scripts/run-stryker.sh` for the baseline-cascade wrapper).

## Local development

The Vite dev server expects a Prometheus instance on
`localhost:9090`. Start one with:

```bash
docker run --rm -d \
  --name prism-prom-dev \
  -p 9090:9090 \
  -v $PWD/apps/prism/e2e/fixtures/prometheus.yml:/etc/prometheus/prometheus.yml:ro \
  prom/prometheus:latest
```

Then `pnpm dev` and open `http://localhost:5173`. Type `up` into the
query input. The fixture self-scrapes Prometheus so the `up` metric
returns a real series.

## Production deployment

Static SPA. `pnpm build` produces `dist/`; the operator's reverse
proxy serves `dist/` and proxies `/api/v1/*` to their Prometheus or
Mimir instance. See ADR-0027 §5 (CORS posture) and
`docs/feature/prism-v0/devops/platform-architecture.md`.

## Licence

AGPL-3.0-or-later. See `scripts/licence-header-agpl.txt` for the
file-level header SSOT and ADR-0032 for the rationale.
