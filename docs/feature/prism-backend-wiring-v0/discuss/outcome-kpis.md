# Outcome KPIs: prism-backend-wiring-v0

## Feature: prism-backend-wiring-v0

### Objective

A paged operator can open Prism in a browser against a running query-api and
see a real metric plotted from the durable Pulse store — the visible payoff of
the ingest -> store -> query loop.

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | A browser-served Prism instance | Mounts the QueryPanel against a valid served config.json (no longer "(unconfigured)") | QueryPanel mounts in 100% of runs; config.json validates against Prism's own loader | Today: 0% — no config.json is served, panel always shows "(unconfigured)" | Prism Vitest/Playwright suite asserting QueryPanel mounted + backend label rendered | Leading |
| 2 | A browser-served Prism instance | Completes a cross-origin (or same-origin) query_range against query-api and renders a real series | 100% of end-to-end runs render >=1 series; CORS allow-origin header present (cross-origin) or single origin (same-origin) | Today: 0% — browser reach to query-api is blocked / untested | End-to-end test (Prism Playwright/Vitest) + query-api backend test asserting reachability headers | Leading |
| 3 | The query_range round-trip | Resolves backend.url + "/query_range" to query-api's /api/v1/query_range and returns a 200 matrix | 100% correct path resolution; 0 spurious 404s from a missing /api/v1 prefix | Today: untested across the browser boundary | End-to-end test asserting a 200 matrix, not a 404 | Leading (correctness) |

### Metric Hierarchy

- **North Star**: Prism's QueryPanel mounts and renders a real series
  end-to-end (config loads + a cross-origin/same-origin query_range succeeds).
- **Leading Indicators**: config.json validates against the real loader (KPI 1);
  browser fetch to query-api succeeds with the reachability mechanism in place
  (KPI 2); path join resolves to a 200 matrix (KPI 3).
- **Guardrail Metrics**: query latency stays within any latency budget against
  ubuntu-latest (no hard SLA pinned at v0 — measured, not gated); header
  redaction invariant (ADR-0027 §6) must not regress; the three config error
  arms must still keep the panel dark on a bad config (no silent mount).

### Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | Prism test suite (Vitest/Playwright) | Assert QueryPanel mounted + real backend label from served config.json | Per CI run on ubuntu-latest | DELIVER wave |
| 2 | Prism end-to-end test + query-api backend test | Assert >=1 series rendered; assert reachability headers (CORS) or single-origin serving | Per CI run | DELIVER wave |
| 3 | End-to-end test | Assert 200 matrix from the real path (no 404) | Per CI run | DELIVER wave |

### Hypothesis

We believe that serving a valid config.json at Prism's origin root and giving a
browser-served Prism an honest way to reach query-api (CORS allow-origin or
same-origin) for a paged operator will achieve a visible, plotted metric from
the durable store. We will know this is true when a browser-served Prism mounts
its QueryPanel and renders a real series end-to-end in CI on ubuntu-latest.

### Handoff to DEVOPS

- Data collection: capture query latency per round-trip against ubuntu-latest
  (guardrail, measured not gated at v0).
- Reachability assertion: the CI environment must run both surfaces — the Prism
  suite (config loads + series renders) AND the query-api side (CORS header
  present, or same-origin serving). The slice has a frontend and a backend
  testable surface.
- No alerting thresholds at v0 (no production SLA pinned). Baseline for KPI 1-3
  is 0% today; the feature establishes the first non-zero baseline.
