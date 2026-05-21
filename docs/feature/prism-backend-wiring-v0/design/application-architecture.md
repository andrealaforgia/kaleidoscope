# Application Architecture: prism-backend-wiring-v0 (Slice 01)

- **Author**: `nw-solution-architect` (Morgan), DESIGN wave, 2026-05-21
- **Mode**: propose
- **Scope**: Slice 01 — a browser-served Prism mounts its QueryPanel and plots
  one metric series read from query-api over the durable Pulse store.
- **Topology**: same-origin via `tower-http` `ServeDir` (DD1). No CORS.
- **Decisions**: see `wave-decisions.md` (DD1-DD7) and ADR-0043.

This section sits under `## Application Architecture` of the platform brief by
reference; it is feature-scoped and does not re-derive platform decisions.

## System Context (C4 L1)

The operator (Priya, on-call SRE) opens Prism in a browser. The browser is the
only external actor. query-api reads the durable Pulse store. No third-party
SaaS, no auth provider, no TLS at v0.

```mermaid
C4Context
  title System Context — prism-backend-wiring-v0 (slice 01, same-origin)
  Person(priya, "Priya", "On-call SRE, incident-time, in a browser")
  System(qapi, "query-api (same-origin server)", "Serves Prism's static bundle AND /api/v1/query_range from one origin")
  SystemDb(pulse, "Pulse store", "Durable file-backed metric store (read-only here)")
  Rel(priya, qapi, "Opens Prism in, then runs PromQL queries against", "HTTP, one origin")
  Rel(qapi, pulse, "Reads metric series from", "MetricStore::query")
```

## Container View (C4 L2)

One origin. The browser fetches `/config.json` and `index.html` (static, served
by `ServeDir`) and `/api/v1/query_range` (the API handler) from the same
query-api process. The API route always wins; any unmatched path falls through
to the static fallback with `index.html` as the SPA fallback.

```mermaid
C4Container
  title Container Diagram — same-origin via ServeDir (slice 01)
  Person(priya, "Priya", "On-call SRE")

  Container_Boundary(origin, "Single origin (one query-api process)") {
    Container(spa, "Prism SPA bundle", "React 19 + Vite + ECharts", "Static dist/: index.html, JS, config.json")
    Container(qapi, "query-api", "Rust, axum 0.7 + tower-http ServeDir", "Routes /api/v1/query_range to the handler; falls back to ServeDir for static assets")
  }
  SystemDb(pulse, "Pulse store", "FileBackedMetricStore (read-only)")

  Rel(priya, spa, "Loads the page from", "GET / -> index.html")
  Rel(spa, qapi, "Fetches config from", "GET /config.json")
  Rel(spa, qapi, "Queries metrics from", "GET /api/v1/query_range?query=..&start=..&end=..&step=15s")
  Rel(qapi, pulse, "Reads series from", "query(tenant, name, range)")
```

`spa` and `qapi` are drawn as distinct containers for clarity, but at runtime
the SPA is *static files served by* qapi's `ServeDir` — they share one process
and one origin. That co-location is precisely what removes CORS.

## Component View (C4 L3) — query-api router

query-api has fewer than five internal components, so a full L3 is not mandated.
This focused L3 shows only the routing seam the feature touches: the additive
`ServeDir` fallback and where it sits relative to the existing API route. The
parser/translator/store internals (ADR-0042) are unchanged and elided.

```mermaid
C4Component
  title Component View — query-api router seam (the only change)
  Container(spa, "Prism SPA (browser)", "React")
  Component(router, "axum Router", "axum 0.7", "Built by router(store, tenant, static_dir)")
  Component(apih, "query_range handler", "Rust async fn", "Existing: parse -> tenant -> query -> matrix")
  Component(serve, "ServeDir fallback", "tower-http fs", "Additive; wired only when static_dir = Some(dir)")
  ComponentDb(pulse, "MetricStore port", "pulse", "query(tenant, name, range)")

  Rel(spa, router, "Sends all requests to", "HTTP, one origin")
  Rel(router, apih, "Routes /api/v1/query_range to", "exact route wins")
  Rel(router, serve, "Falls back unmatched paths to", "static + index.html SPA fallback")
  Rel(apih, pulse, "Reads series from", "MetricStore::query")
  Rel(serve, spa, "Serves /config.json + index.html + JS to", "200 file body")
```

When `static_dir = None` (default), the `ServeDir` component is absent and the
router is byte-for-byte today's API-only router. The read-only `query-range-api-v0`
behaviour does not regress.

## Request flow — the read loop made visible

```mermaid
sequenceDiagram
  participant B as Browser (Prism)
  participant Q as query-api (one origin)
  participant P as Pulse store
  B->>Q: GET / (same origin)
  Q-->>B: 200 index.html (ServeDir SPA fallback)
  B->>Q: GET /config.json
  Q-->>B: 200 {backend:{url:"/api/v1",label:"Pulse (durable)"},prism:{version:"0.1.0"}}
  Note over B: loadConfig -> {kind:'ok'}; QueryPanel mounts
  B->>Q: GET /api/v1/query_range?query=up&start=..&end=..&step=15s
  Note over Q: API route wins over ServeDir fallback
  Q->>P: query(tenant, "up", [start,end))
  P-->>Q: Vec<(Metric, MetricPoint)>
  Q-->>B: 200 {status:"success",data:{resultType:"matrix",result:[...]}}
  Note over B: one series plotted; footer "1 series • N points • M ms"
```

The browser never crosses an origin, so no preflight `OPTIONS` and no
`Access-Control-Allow-Origin` appear in this flow. That absence is the point:
the simplest honest mechanism removes a failure class rather than configuring
it.

## The `backend.url` `/api/v1` reconciliation (visible in the flow)

`buildUrl` joins `${backend.url}/query_range`. With the committed
`backend.url = "/api/v1"`, the browser issues `GET /api/v1/query_range`,
resolved against the page's own origin. query-api's `QUERY_RANGE_ROUTE` is
`/api/v1/query_range`. The join lands on the route -> 200 matrix, not a 404.
ADR-0027 §5's prose (which read `${backend.url}/api/v1/query_range`) is corrected
by ADR-0043 to match the shipped `buildUrl`: the `/api/v1` lives *inside*
`backend.url`.

## Quality attributes (ISO 25010, slice-scoped)

| Attribute | How addressed |
|---|---|
| Functional suitability | config.json validates against Prism's real loader (3 error arms preserved); 200 matrix resolves, not a 404; empty result is the calm empty arm |
| Performance efficiency | no CORS preflight on the incident-time path (one round-trip per query); query latency captured as a guardrail (measured, not gated at v0) |
| Compatibility | one origin, one cert, one log stream (ADR-0027 §5 posture); the existing matrix JSON contract is unchanged |
| Reliability | Earned-Trust probe unchanged (wire-then-probe-then-use): query-api refuses to start if the store cannot be read or no tenant resolves (`health.startup.refused`) |
| Security | fail-closed tenancy preserved (unresolved tenant -> 401 status:error); header-redaction invariant (ADR-0027 §6) untouched; no auth/TLS introduced (out of scope, documented) |
| Maintainability | additive, behind-config `ServeDir`; default-off so the API-only path is unchanged; testable via `oneshot` (no port bound) |
| Portability | origin-relative `backend.url` makes the same bundle portable across hosts under same-origin serving |

## Earned-Trust posture (principle 12)

The new `ServeDir` fallback is a driven adapter onto the filesystem (it reads
`dist/` and serves bytes). It does not change the composition root's existing
probe: `composition::probe` still asserts the store is readable and a tenant
resolves before the listener binds. The `ServeDir` itself is exercised
empirically by the slice-01 RED test (DD6): a `oneshot` against a temp dir
asserts `/config.json` and `index.html` are actually served and that the API
route wins over the static fallback — the adapter demonstrates it can honour its
contract against a real (temp) filesystem rather than by convention. If the
static dir is misconfigured (path absent), `ServeDir` returns 404 for static
paths and the API path still works; the browser's loader then surfaces
`fetch-failed` and keeps the panel dark (correct refusal, not a silent mount).
```
