# KPI Instrumentation: prism-backend-wiring-v0 (DEVOPS)

- **Author**: `nw-platform-architect` (Apex). Date: 2026-05-21.
- **DISCUSS basis**: `discuss/outcome-kpis.md`. DESIGN basis: ADR-0043, DD6.
- **Note on data collection**: every KPI is measured by an EXISTING CI gate.
  No new instrumentation, no telemetry pipeline, no alerting threshold is
  added at v0 (no production SLA pinned). The feature establishes the first
  non-zero baseline (baseline today is 0% for all three KPIs).

## North Star

Prism's QueryPanel mounts and renders a real series end-to-end (config loads
and a same-origin `query_range` succeeds). This maps onto a backend surface
(query-api ServeDir + route precedence) and a frontend surface (prism's mount
and E2E suites), both verified per CI run on ubuntu-latest.

## KPI -> gate mapping

| KPI | What it asserts | Measured by (existing gate) |
|---|---|---|
| 1 (leading) | QueryPanel mounts against a valid served config.json; three error arms keep it dark on a bad config | Gate 6 (Vitest): `App.tsx` `fetchFn` seam injects the slice-01 config -> mounts with label "Pulse (durable)", version "0.1.0"; bad shape -> `shape-failed`; 404 -> `fetch-failed` |
| 2 (leading) | Browser renders >=1 series same-origin end-to-end | Gate 7 (Playwright E2E) + Gate 11 (Prometheus contract): same-origin query-api serving `dist/` + `/api/v1`, `KALEIDOSCOPE_QUERY_TENANT` set |
| 3 (leading, correctness) | `backend.url` + `/query_range` resolves to `/api/v1/query_range` -> 200 matrix, not a 404 | Gate 11 end-to-end + the query-api `oneshot` test asserting the API route wins over the static fallback (DD6) |

## Backend instrumentation (query-api, Gate 1 + Gate 5)

The new behaviour is pinned by the DD6 RED tests, discovered by Gate 1
(`cargo test --all-targets`) and mutation-killed by `gate-5-mutants-query-api`
(`--package query-api --in-diff`):

1. `GET /config.json` -> 200 with the file body (ServeDir serves it).
2. `GET /` (or unknown SPA path) -> 200 `index.html` (SPA fallback).
3. `GET /api/v1/query_range?...` -> the API handler (matrix / `status:error`
   shape), NOT an `index.html` body -- the API route WINS over the static
   fallback. This is the correctness round-trip for KPI 3.
4. With `router(store, tenant, None)` -> `GET /config.json` returns 404
   (default-off; query-range-api-v0 behaviour unchanged).
5. `composition::resolve_static_dir` precedence (unset/empty -> `None`) -- a
   pure unit test, mutation-killed without binding a port.

## Frontend instrumentation (prism, Gates 6/7/11)

- **Correctness round-trip (KPI 3)**: the committed `config.json` validates
  against prism's real loader `isRuntimeConfig`
  (`apps/prism/src/lib/config/loader.ts`) -- `backend` and `prism` are objects;
  `backend.url`, `backend.label`, `prism.version` are strings. This is
  asserted by Gate 6, not by a separate schema tool: the loader IS the
  contract.
- **Mount (KPI 1)**: Gate 6 Vitest against the `fetchFn` seam.
- **Series render (KPI 2 / North Star)**: Gate 7 Playwright + Gate 11
  contract fixture, footer "1 series, N points, M ms".

## Guardrails (measured, not gated at v0)

- **Query latency** per round-trip on ubuntu-latest -- captured as a
  guardrail; no hard SLA pinned at v0 (per `outcome-kpis.md`). No alert.
- **Header-redaction invariant** (ADR-0027 section 6) must not regress --
  covered by prism's existing redaction tests (Gate 6); unchanged here.
- **No silent mount**: the three config error arms must still keep the panel
  dark on a bad config -- Gate 6 negative cases (Scenarios 2 and 3).

## Dashboards / alerting

None at v0. The "dashboard" is the CI gate verdict on ubuntu-latest: green
across Gates 1, 5, 6, 7, 11 IS the North Star verification. Baseline is 0%
today; this feature establishes the first non-zero reading. No production
alerting threshold until a real SLA is pinned (future wave).
