# Slice 01: Operator sees a metric plotted in a browser

Feature: `prism-backend-wiring-v0`. The minimal end-to-end wiring that gets the
QueryPanel mounted and one metric plotted in a browser-equivalent test.

## Stories

- US-01: The QueryPanel mounts against a valid served config.json.
- US-02: A browser-served Prism reaches query-api and plots a real series.

Both ship together. Neither delivers operator value alone.

## Elevator Pitch (slice level)

An operator opens Prism in a browser, the QueryPanel mounts (no longer
"unconfigured"), enters a metric name, and sees its series plotted, served from
query-api over the durable Pulse store.

## Honest scope boundary

In scope: a valid config.json served at Prism's origin root + the chosen
browser-reachability mechanism (CORS allow-origin on query-api as the thinnest
option, or same-origin serving) + proof end-to-end that the panel mounts and
one series renders.

Out of scope (later/v1): auth, TLS, multi-origin allowlists, full deploy
orchestration.

## Central design fork (decide in DESIGN)

1. Cross-origin + CORS: Prism on origin A, query-api on origin B with a
   configurable CORS allow-origin layer. Decoupled; needs CORS config.
2. Same-origin: one server serves both Prism's bundle and /api/v1. No CORS;
   couples the deployment.

Capture both with tradeoffs; do NOT pick in DISCUSS.

## Testable surfaces

- Frontend (Prism Vitest/Playwright): QueryPanel mounts from served config.json;
  a real series renders end-to-end; empty + transport-error arms behave.
- Backend (query-api): reachability headers present (CORS allow-origin) or
  same-origin serving; 200 matrix from /api/v1/query_range; fail-closed tenancy
  set in the test environment.

## North-Star acceptance

A browser-served Prism mounts its QueryPanel and renders a real series
end-to-end (config loads + a cross-origin/same-origin query_range succeeds), on
ubuntu-latest in CI.

## Key contract facts (verified)

- config.json shape: `{ backend: { url: string, label: string }, prism: { version: string } }`
  validated by `apps/prism/src/lib/config/loader.ts`.
- Path join: `buildUrl` -> `${backend.url}/query_range`; query-api serves
  `/api/v1/query_range`; backend.url MUST carry `/api/v1`.
- query-api has NO CORS today; tower-http is in the workspace lock, not a direct
  query-api dependency.
