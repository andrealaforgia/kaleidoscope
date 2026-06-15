# ADR-0078 — Cross-signal same-origin SPA gateway: the metrics/SPA origin also serves the trace query routes

- **Status**: Accepted
- **Date**: 2026-06-15
- **Author**: `nw-software-crafter` (Crafty)
- **Feature**: `experimentable-stack-v0` (a DELIVER-time wiring correction)
- **Supersedes**: none
- **Superseded by**: none
- **Related**: extends ADR-0077 F4 (same-origin Prism on the metrics router) to
  the trace signal; builds on ADR-0043 (same-origin via `tower-http` `ServeDir`,
  the no-CORS posture); ADR-0048 / ADR-0053 (the `/api/v1/traces*` route
  contracts); ADR-0076 (the consolidated runtime composition root).

## Context

ADR-0077 F4 made the consolidated runtime serve Prism's built bundle from the
**same origin** as the metrics query API on port 9090 (via
`KALEIDOSCOPE_QUERY_STATIC_DIR` → a `tower-http` `ServeDir` fallback on the
metrics router). Prism's relative `backend.url` (`/api/v1`) is answered
same-origin, so `/api/v1/query_range` works with no CORS.

But the **trace** query routes (`/api/v1/traces`, `/api/v1/traces/by_id`,
`/api/v1/traces/with_logs`) were mounted ONLY on the standalone traces listener
(:9092). Verified live on a running instance:

- `GET :9090/api/v1/traces/with_logs?trace_id=...` returns **200 text/html** —
  the SPA `index.html` static fallback, NOT the trace JSON — because no
  `/api/v1/traces*` route existed on the metrics router, so the request fell
  through to the `ServeDir` `index.html` fallback.
- A cross-origin call from the SPA origin (:9090) to :9092 fails the same-origin
  policy: an `OPTIONS` preflight returns 405 with no `Access-Control-*` headers
  (the traces router has no CORS, by ADR-0043's no-CORS posture).

So the SPA, as served in production, could not reach the trace data its upcoming
"linked view" screen needs. The metrics signal worked same-origin; the trace
signal did not. This is a wiring gap, not a new product surface.

## Decision

**Mount the trace query routes on the metrics/SPA router (:9090) too**, sharing
the SAME trace store and log store `Arc`s the standalone :9092 traces router
already uses, so the SPA reaches the trace routes same-origin with relative
paths and no CORS — exactly the ADR-0077 F4 / ADR-0043 posture, now applied
across signals.

Concretely, in `spawn_consolidated` (`crates/kaleidoscope-runtime/src/lib.rs`):

```text
let spa_trace_routes = trace_query_api::router_with_auth_and_logs(
    Arc::clone(&trace_dyn), Arc::clone(&log_dyn), traces_tenant.clone(), read_auth.clone());

let metrics_router = query_api::router_with_auth(
        metric_dyn, metrics_tenant, read_auth.clone(), static_dir.clone())
    .merge(spa_trace_routes);
```

Three properties make this correct and minimal:

1. **Reuse, no duplication.** The trace routes for :9090 are built through the
   SAME `trace_query_api::router_with_auth_and_logs` constructor the standalone
   :9092 listener uses — it already returns a mergeable `axum::Router` with no
   listener bound. No handler logic is duplicated; no new constructor is
   introduced.
2. **Same data, shared store.** The merged routes hold `Arc::clone`s of the same
   `trace_dyn` / `log_dyn` allocations (same interior `Mutex`) the :9092 router
   and the ingest sink share, so a span/log ingested at T is visible on both
   origins at T+ε (the ADR-0076 shared-`Arc` invariant, unchanged).
3. **Route precedence over the SPA fallback.** An exact axum `.route(..)` always
   wins over a `.fallback_service(..)`, so `/api/v1/traces*` matches the merged
   trace routes while every OTHER unmatched non-API path still falls through to
   the SPA `index.html`. `/api/v1/query_range` and `/help` are unchanged. There
   is no route collision (the metrics and trace route sets are disjoint), and
   only the metrics router carries a custom fallback, so the merge keeps the SPA
   fallback intact.

The standalone :9092 traces listener is **unchanged** — direct API consumers are
untouched. No CORS is introduced anywhere.

## Alternatives considered

### A1 (rejected) — CORS on the per-signal traces port (:9092)

Add a `tower-http` `CorsLayer` to the standalone traces router allowing the SPA
origin, so the browser calls :9092 cross-origin. **Rejected**: it reintroduces
the exact failure class ADR-0043 and ADR-0077 A1 already rejected for the
metrics signal — a preflight round-trip on the incident-time path, an
allow-origin config surface that must track each deployment's SPA origin, and a
class of allow-origin-mismatch / credentialed-request failures. It would also
make the trace signal inconsistent with the metrics signal (same-origin) for no
benefit. Same-origin needs no per-deployment origin config, no preflight, and
matches the established metrics pattern.

### A2 (rejected) — a Prism config edit to point trace calls at an absolute :9092 URL

Give Prism an absolute trace backend URL. **Rejected**: it forces a
per-host `config.json` edit (against the relative-URL default), splits the
SPA's backend into two origins, and still needs CORS on :9092. Strictly worse
than serving same-origin.

## Consequences

### Positive

- The SPA reaches all of its trace surfaces (`/api/v1/traces`,
  `/api/v1/traces/by_id`, `/api/v1/traces/with_logs`) same-origin with relative
  paths — the linked view can be built with no CORS and no Prism config change.
- Consistent with ADR-0077 F4 / ADR-0043: one origin, no CORS, no preflight,
  across metrics AND traces.
- Minimal and additive: one composition-root change reusing existing
  constructors and the already-shared stores; zero new handler logic, zero new
  ports, zero change to the standalone :9092 router.

### Negative / trade-offs

- The metrics/SPA router now also depends (at composition) on the trace and log
  stores. This was already true of the runtime as a whole (it builds all three
  stores and the :9092 traces router from them); the `Arc::clone` adds a second
  cheap reference to the same allocation, not a second store.

## Verification

- Runtime acceptance test
  `crates/kaleidoscope-runtime/tests/slice_05_spa_origin_trace_routes.rs`
  (`metrics_spa_origin_also_serves_trace_query_routes_same_origin`): ingest an
  error span + correlated log + a metric over the REAL ingest path on EPHEMERAL
  `127.0.0.1:0` ports with a REAL temp Prism bundle, then assert ON THE METRICS
  ORIGIN that (1) `/api/v1/traces/with_logs` returns the trace JSON carrying both
  the span and the log, (2) `/api/v1/traces?...&error=true` returns the failed
  trace, (3) `/api/v1/query_range` still returns the metric, (4) an unknown
  non-API path still returns the SPA `index.html`. FALSIFIABILITY: before the
  merge, (1)/(2) fall through to the `index.html` fallback and the JSON parse
  fails — a regression-free RED isolating exactly the new behaviour.
- The standalone :9092 traces routes and their existing tests
  (`crates/trace-query-api/tests/`) stay green (unchanged).
- Mutation testing: `cargo mutants --in-diff` scoped to the modified
  `crates/kaleidoscope-runtime/src/lib.rs`, 100% kill rate.
